use std::collections::BTreeMap;

use anyhow::Result;
use async_once_cell::OnceCell;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use k8s_openapi::api::{
    batch::v1::{Job, JobSpec, JobStatus},
    core::v1::{Container, PodSpec, PodTemplateSpec, Pod},
};
use kube::{
    api::{ListParams, ObjectMeta, PostParams, LogParams},
    runtime::watcher,
    Api, Client,
};
use bytes::Bytes;

use crate::db_object::Build;

pub struct K8S;

static CLIENT: OnceCell<Client> = OnceCell::new();
static JOBS: OnceCell<Api<Job>> = OnceCell::new();
static PODS: OnceCell<Api<Pod>> = OnceCell::new();

impl K8S {
    async fn client() -> Client {
        CLIENT
            .get_or_init(async { Client::try_default().await.unwrap() })
            .await
            .clone()
    }

    async fn jobs() -> &'static Api<Job> {
        JOBS.get_or_init(async { Api::default_namespaced(K8S::client().await) })
            .await
    }

    async fn pods() -> &'static Api<Pod> {
        PODS.get_or_init(async { Api::default_namespaced(K8S::client().await) })
            .await
    }
}

pub async fn dispatch_build(id: String, image: String) -> Result<()> {
    let jobs = K8S::jobs().await;

    // TODO: Issue a build token and pass it into the job

    let mut labels = BTreeMap::new();
    labels.insert("anda-build-id".to_string(), id.clone());

    let spec = Job {
        metadata: ObjectMeta {
            name: Some(format!("build-{}", id)),
            labels: Some(labels),
            ..ObjectMeta::default()
        },
        spec: Some(JobSpec {
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers: vec![Container {
                        image: Some(image),

                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                metadata: Some(ObjectMeta {
                    name: Some(format!("build-pod-{}", id)),
                    ..ObjectMeta::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };

    jobs.create(&PostParams::default(), &spec).await?;

    Ok(())
}

pub enum BuildStatusEvent {
    Running(String),
    Succeeded(String),
    Failed(String),
}

async fn watch_jobs() -> impl Stream<Item = BuildStatusEvent> {
    let jobs = K8S::jobs().await;
    let stream = watcher(jobs.clone(), ListParams::default());

    stream.flat_map(|e| match e {
        Ok(watcher::Event::Applied(job)) => {
            if let Some(labels) = job.metadata.labels {
                if let Some(id) = labels.get("anda-build-id") {
                    if let Some(status) = job.status {
                        if let Some(1) = status.active {
                            return stream::iter(vec![BuildStatusEvent::Running(id.clone())]);
                        }

                        if let Some(1) = status.failed {
                            return stream::iter(vec![BuildStatusEvent::Failed(id.clone())]);
                        }

                        if let Some(1) = status.succeeded {
                            return stream::iter(vec![BuildStatusEvent::Succeeded(id.clone())]);
                        }
                    }
                }
            };

            stream::iter(vec![])
        }
        _ => stream::iter(vec![]),
    })
}

pub async fn get_logs(id: String) -> Result<impl Stream<Item = Result<Bytes, kube::Error>>, kube::Error> {
    let jobs = K8S::jobs().await;
    let pods = K8S::pods().await;
    
    // let job = jobs.get(format!("build-{}", id).as_str()).await?;
 
    // if let Some(spec) = job.spec {
        // if let Some(template) = spec.template {
            // template.
        // }
        
    // }

    pods.log_stream(format!("build-pod-{}", id).as_str(), &LogParams {
        follow: true,
        ..Default::default()
    }).await
}
