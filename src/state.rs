use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use hyper::{service::{make_service_fn, service_fn}, Server, Request, Body, Response};
use k8s_openapi::api::core::v1::{Service, Pod};
use kube::{Api, Client, api::ListParams, ResourceExt, Error};
use tokio::sync::{Mutex, mpsc::Sender};
use tui::widgets::ListState;

pub struct AppState {
    pub namespace_opt: Option<String>,
    pub ports_by_service: BTreeMap<String, Vec<i32>>,
    pub service_selection: ListState,
    pub port_selection: ListState,
    pub forwarded_ports: Vec<ForwardedPort>,
}

pub struct ForwardedPort {
    pub service: String,
    pub port: u16,
    pub sender: Sender<()>
}

impl AppState {
    pub async fn new(namespace_opt: Option<String>) -> Result<AppState, Error> {
        let services = AppState::get_services(&namespace_opt).await?;
        let ports_by_service: BTreeMap<String, Vec<i32>> = services
        .iter()
        .map(|svc| 
            (
                svc.metadata.name.to_owned().unwrap(), 
                svc.clone().spec.unwrap().ports.unwrap().iter().map(|port| port.port).collect::<Vec<i32>>()
            )
        )
        .collect();

        Ok(AppState { 
            namespace_opt,
            ports_by_service,
            service_selection: ListState::default(), 
            port_selection: ListState::default(),
            forwarded_ports: vec![]
        })
    }

    pub fn forwarded_ports_for_service(&self, service: &str) -> Vec<&ForwardedPort> {
        self.forwarded_ports
            .iter()
            .filter(|fw_port| fw_port.service == service).collect()
    }

    pub fn forwarded_ports_for_selected_service(&self) -> Vec<&ForwardedPort> {
        if let Some(service) = self.service() {
            self.forwarded_ports_for_service(&service)
        } else {
            vec![]
        }
    }

    pub fn service(&self) -> Option<String> {
        if let Some(selected_service) = self.service_selection.selected() {
            Some(self.service_list()[selected_service].clone())
        } else {
            None
        }
    }

    pub fn service_list(&self) -> Vec<String> {
        self.ports_by_service
            .keys()
            .into_iter().map(|service| service.to_owned())
            .collect()
    }

    pub fn port_list(&self) -> Vec<i32> {
        if let Some(selected_service) = self.service_selection.selected() {
            self.ports_by_service
                .values()
                .map(|port| port.to_owned())
                .collect::<Vec<Vec<i32>>>()[selected_service]
                .to_owned()
        } else {
            vec![]
        }
    }

    pub fn select(&mut self) {
        if self.port_selection.selected().is_none() {
            if !self.port_list().is_empty() {
                self.port_selection.select(Some(0));
            }
        }
    }

    pub fn deselect(&mut self) {
        if let Some(_) = self.port_selection.selected() {
            self.port_selection.select(None);
        }
    }

    pub fn next(&mut self) {
        if let Some(selected_port) = self.port_selection.selected() {
            self.port_selection.select(Some((selected_port + 1) % self.port_list().len()));
        } else if let Some(selected_service) = self.service_selection.selected() {
            self.service_selection.select(Some((selected_service + 1) % self.service_list().len()));
        } else if !self.service_list().is_empty() {
            self.service_selection.select(Some(0));
        }
    }

    pub fn previous(&mut self) {
        if let Some(selected_port) = self.port_selection.selected() {
            let port_list_len = self.port_list().len() as i32;
            self.port_selection.select(Some((((selected_port as i32 - 1) + port_list_len) % port_list_len) as usize));
        } else if let Some(selected_service) = self.service_selection.selected() {
            let svc_list_len = self.service_list().len() as i32;
            self.service_selection.select(Some((((selected_service as i32 - 1) + svc_list_len) % svc_list_len) as usize));
            self.port_selection.select(None);
        } else if !self.service_list().is_empty() {
            self.service_selection.select(Some(self.service_list().len() - 1));
        }
    }

    pub async fn toggle_port_forwarding(&mut self) -> Result<(), kube::Error> {
        if let Some(selected_port) = self.port_selection.selected() {
            let selected_svc = &self.service_list()[self.service_selection.selected().unwrap()];
            let selected_port = self.port_list()[selected_port] as u16;
            let forwarded_ports = &mut self.forwarded_ports;
            if let Some(existing_forwarded_port_idx) = forwarded_ports.into_iter().position(|port| {
                &port.service == selected_svc && port.port == selected_port
            }) {
                let existing_forwarded_port = &forwarded_ports[existing_forwarded_port_idx];
                if let Ok(()) = existing_forwarded_port.sender.send(()).await {
                    forwarded_ports.remove(existing_forwarded_port_idx);
                }
                Ok(())
            } else {
                if let Some(sender) = AppState::run_port_forward(&self.namespace_opt, &selected_svc, selected_port).await? {
                    self.forwarded_ports.push(ForwardedPort { service: selected_svc.clone(), port: selected_port, sender });
                }
                Ok(())
            }
        } else {
            let selected_svc = &self.service_list()[self.service_selection.selected().unwrap()];
            let all_svc_ports = &self.ports_by_service[selected_svc];
            let svc_forwarded_ports = self.forwarded_ports_for_selected_service();
            let should_stop_port_forwarding = all_svc_ports.len() == svc_forwarded_ports.len();

            if should_stop_port_forwarding {
                let forwarded_ports_futs: Vec<_> = svc_forwarded_ports.iter().map(|port| async { 
                    port.sender.send(()).await 
                }).map(Box::pin).collect();
                if let (Err(error), _, _) = futures::future::select_all(forwarded_ports_futs).await {
                    println!("An error occurred stopping port forwarding for service '{}': {}", selected_svc, error);
                    return Ok(());
                }
                self.forwarded_ports.retain(|port| &port.service != selected_svc);
            } else {
                let namespace_opt = &self.namespace_opt;
                let forwarded_ports = Arc::new(Mutex::new(&mut self.forwarded_ports));
                let forwarded_ports_futs: Vec<_> = all_svc_ports.iter().map(|port| async {
                    match AppState::run_port_forward(namespace_opt, &selected_svc, port.to_owned() as u16).await {
                        Ok(Some(sender)) => {
                            forwarded_ports.lock().await.push(ForwardedPort { service: selected_svc.clone(), port: (port.to_owned() as u16), sender })
                        }
                        Err(error) => {
                            println!("An error occurred forwarding port {} for service {}: {}", port.to_owned(), selected_svc.clone(), error.to_string());
                        }
                        _ => ()
                    }
                }).map(Box::pin).collect();

                futures::future::select_all(forwarded_ports_futs).await;
            }
            Ok(())
        }
    }

    async fn get_services(namespace_opt: &Option<String>) -> Result<Vec<Service>, kube::Error> {
        let client = Client::try_default().await?;
        let service_api: Api<Service> = if let Some(ns) = namespace_opt {
            Api::namespaced(client, &ns)
        } else {
            Api::default_namespaced(client)
        };
        let services: Vec<Service> = service_api.list(&ListParams::default()).await?.items;
        Ok(services)
    }

    async fn run_port_forward(namespace_opt: &Option<String>, service: &str, port: u16) -> Result<Option<Sender<()>>, kube::Error> {
        let client = Client::try_default().await?;
        let pod_api: Api<Pod> = if let Some(ns) = namespace_opt {
            Api::namespaced(client, &ns)
        } else {
            Api::default_namespaced(client)
        };
        let pod_opt = pod_api
            .list(&ListParams::default())
            .await
            .iter()
            .flat_map(|pods| pods.items.to_owned())
            // FIXME: This looks for a pod whose name has the service 
            //        name as prefix and might select an unrelated pod.
            .find(|pod| pod.name().starts_with(service));
    
        if let Some(pod) = pod_opt {
            let mut port_forwarder = pod_api.portforward(&pod.name(), &vec![port]).await?;
            let stream = port_forwarder.take_stream(port).unwrap();
            let (sender, connection) = hyper::client::conn::handshake(stream).await.unwrap();
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    println!("Connection on port {} failed: {}", port, e);
                }
            });
            
            tokio::spawn(async move {
                if let Err(e) = port_forwarder.join().await {
                    println!("Port forwarding for port {} on service {} failed: {}", port, &pod.name(), e);
                }
            });
    
            let handle_request = |
                context: Arc<Mutex<hyper::client::conn::SendRequest<hyper::Body>>>,
                req: Request<Body>| async move {
                let sender = context.lock();
                let response = sender.await.send_request(req).await?;
                Ok(response) as Result<Response<Body>, hyper::Error>
            };
            let context = Arc::new(Mutex::new(sender));
            let make_service = make_service_fn(move |_conn| {
                let context = context.clone();
                let service = service_fn(move |req| handle_request(context.clone(), req));
                async move { Ok::<_, hyper::Error>(service) }
            });
    
            let (sender, mut rx) = tokio::sync::mpsc::channel(1);
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
    
            tokio::spawn(async move {
                let server = Server::bind(&addr)
                .serve(make_service)
                .with_graceful_shutdown(async {
                    rx.recv().await;
                });
    
                if let Err(e) = server.await {
                    println!("server error: {}", e);
                }
            });
    
            return Ok(Some(sender));
        }
    
        Ok(None)
    }
}