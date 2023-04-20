use std::collections::BTreeMap;

use anyhow::Context;
use k8s_openapi::{
    api::core::v1::{ContainerPort, Pod, Service},
    apimachinery::pkg::util::intstr::IntOrString,
};
use kube::{
    api::{ListParams, Portforwarder},
    Api, Client,
};
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Debug, thiserror::Error)]
pub enum Errors {
    #[error("Pod Not Found {namespace}/{pod}")]
    PodNotFound { namespace: String, pod: String },
    #[error("Service Not Found {namespace}/{service}")]
    ServiceNotFound { namespace: String, service: String },
    #[error("Service {namespace}/{service} Invalid - {reason}")]
    ServiceInvalid {
        namespace: String,
        service: String,
        reason: String,
    },
    #[error("Service {namespace}/{service} has not matching ready pods")]
    ServiceNoReadyPods { namespace: String, service: String },
    #[error("Pod {pod} for service {namespace}/{service} not found")]
    NamedServicePodsNotFound {
        namespace: String,
        service: String,
        pod: String,
    },
    #[error("Port {2} Not Found on {0}/{1}")]
    PortNotFound(String, String, u16),
    #[error("Unsupported Address {0}")]
    UnsupportedAddress(String),
    #[error("Forward Failed {0:?}")]
    ForwardFailed(#[source] anyhow::Error),
    #[error("Lookup Failed {0:?}")]
    LookupFailed(#[source] kube::Error),
}

pub struct PodResolver {
    client: Client,
    forwarder: Option<Portforwarder>,
}

impl PodResolver {
    pub fn new(client: Client) -> Self {
        PodResolver {
            client,
            forwarder: None,
        }
    }

    pub async fn forwarder(
        &mut self,
        address: &str,
        port: u16,
    ) -> Result<impl AsyncRead + AsyncWrite + Unpin, Errors> {
        let (pod_name, namespace, port) = self.resolve(address, port).await?;

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace.as_str());

        let mut forwarder = pods
            .portforward(pod_name.as_str(), &[port])
            .await
            .map_err(|e| Errors::ForwardFailed(e.into()))?;

        let stream = forwarder
            .take_stream(port)
            .context("port not found in forwarder")
            .map_err(Errors::ForwardFailed)?;

        self.forwarder = Some(forwarder);

        Ok(stream)
    }

    pub async fn join(self) -> anyhow::Result<()> {
        if let Some(f) = self.forwarder {
            f.join().await?
        }

        Ok(())
    }

    async fn resolve(&self, address: &str, port: u16) -> Result<(String, String, u16), Errors> {
        let mut segments: Vec<&str> = address.split('.').collect();

        if let Some(mut segment) = segments.pop() {
            if segment == "local" && segments.last() == Some(&"cluster") {
                let _ = segments.pop(); // remove "cluster"

                // grab the now last dns segment for the sub-resolver matching below
                segment = segments
                    .pop()
                    .ok_or_else(|| Errors::UnsupportedAddress(address.to_string()))?;
            }

            return match segment {
                "svc" => self.resolve_service(segments.as_slice(), port).await,
                "pod" => self.resolve_pod(segments.as_slice(), port).await,
                _ => Err(Errors::UnsupportedAddress(address.to_string())),
            };
        }

        Err(Errors::UnsupportedAddress(address.to_string()))
    }

    async fn resolve_service(
        &self,
        segments: &[&str],
        port: u16,
    ) -> Result<(String, String, u16), Errors> {
        let pod_hostname: Option<&str>;
        let service_name: &str;
        let namespace: &str;

        if segments.len() == 2 {
            pod_hostname = None;
            service_name = segments[0];
            namespace = segments[1];
        } else if segments.len() == 2 {
            pod_hostname = Some(segments[0]);
            service_name = segments[1];
            namespace = segments[2];
        } else {
            return Err(Errors::UnsupportedAddress(
                segments.join(".") + "svc.cluster.local",
            ));
        }

        let service_api: Api<Service> = Api::namespaced(self.client.clone(), namespace);
        let pod_api: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        if let Some(service) = service_api
            .get_opt(service_name)
            .await
            .map_err(Errors::LookupFailed)?
        {
            let selectors = service
                .spec
                .as_ref()
                .ok_or_else(|| Errors::ServiceInvalid {
                    namespace: namespace.into(),
                    service: service_name.into(),
                    reason: "spec is not set".into(),
                })?
                .selector
                .as_ref()
                .ok_or_else(|| Errors::ServiceInvalid {
                    namespace: namespace.into(),
                    service: service_name.into(),
                    reason: "spec.selectors is not set".into(),
                })?;

            let list_params = selector_into_list_params(selectors);

            let pods = pod_api
                .list(&list_params)
                .await
                .map_err(Errors::LookupFailed)?;

            if let Some(hostname) = pod_hostname {
                if let Some(pod) = pods.items.iter().find(|p| {
                    Some(&hostname.into())
                        == p.spec
                            .as_ref()
                            .and_then(|s| s.hostname.as_ref())
                            .or(p.metadata.name.as_ref())
                }) {
                    return Ok((pod.metadata.name.clone().unwrap(), namespace.into(), port));
                } else {
                    return Err(Errors::NamedServicePodsNotFound {
                        namespace: namespace.into(),
                        service: service_name.into(),
                        pod: hostname.into(),
                    });
                }
            }

            let ready_pod = pods.items.iter().find(|p| {
                p.status.as_ref().map_or(false, |s| {
                    s.conditions.as_ref().map_or(false, |cs| {
                        cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                    })
                })
            });

            if let Some(pod) = ready_pod {
                let service_port = service
                    .spec
                    .as_ref()
                    .and_then(|s| s.ports.iter().flatten().find(|p| p.port == port as i32))
                    .and_then(|p| p.target_port.clone());

                let pod_port = match service_port {
                    Some(IntOrString::String(port_name)) => pod
                        .spec
                        .as_ref()
                        .and_then(|spec| {
                            spec.containers
                                .iter()
                                .flat_map(|c| c.ports.as_ref().unwrap_or(EMPTY_CONTAINER_PORT_VEC))
                                .find(|p| p.name.as_ref().is_some_and(|n| n == &port_name))
                                .and_then(|p| u16::try_from(p.container_port).ok())
                        })
                        .ok_or_else(|| {
                            Errors::PortNotFound(namespace.into(), service_name.into(), port)
                        }),
                    Some(IntOrString::Int(i)) => {
                        u16::try_from(i).map_err(|_| Errors::ServiceInvalid {
                            namespace: namespace.into(),
                            service: service_name.into(),
                            reason: "could not convert target port to u16".into(),
                        })
                    }
                    None => Ok(port),
                }?;

                return Ok((
                    pod.metadata.name.clone().unwrap(),
                    namespace.into(),
                    pod_port,
                ));
            } else {
                return Err(Errors::ServiceNoReadyPods {
                    namespace: namespace.into(),
                    service: service_name.into(),
                });
            }
        }

        Err(Errors::ServiceNotFound {
            namespace: namespace.into(),
            service: service_name.into(),
        })
    }

    async fn resolve_pod(
        &self,
        segments: &[&str],
        port: u16,
    ) -> Result<(String, String, u16), Errors> {
        if segments.len() != 2 {
            return Err(Errors::UnsupportedAddress(
                segments.join(".") + "pod.cluster.local",
            ));
        }

        let pod_name = segments[0];
        let namespace = segments[1];

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        if let Some(pod) = pods.get_opt(pod_name).await.map_err(Errors::LookupFailed)? {
            // todo try and find port on pod or error
        } else {
            return Err(Errors::PodNotFound {
                namespace: namespace.into(),
                pod: pod_name.into(),
            });
        }

        Ok((pod_name.into(), namespace.into(), port))
    }
}

const EMPTY_CONTAINER_PORT_VEC: &Vec<ContainerPort> = &Vec::new();

fn selector_into_list_params(selectors: &BTreeMap<String, String>) -> ListParams {
    let labels = selectors
        .iter()
        .fold(String::new(), |mut res, (key, value)| {
            if !res.is_empty() {
                res.push(',');
            }
            res.push_str(key);
            res.push('=');
            res.push_str(value);
            res
        });

    ListParams::default().labels(&labels)
}
