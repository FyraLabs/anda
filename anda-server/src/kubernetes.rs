use std::collections::BTreeMap;

use anyhow::Result;
use async_once_cell::OnceCell;
use bytes::Bytes;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use k8s_openapi::api::{
    batch::v1::{Job, JobSpec},
    core::v1::{Container, EnvVar, Pod, PodSpec, PodTemplateSpec},
};
use kube::{
    api::{ListParams, LogParams, ObjectMeta, PostParams},
    core::WatchEvent,
    runtime::watcher,
    Api, Client, ResourceExt,
};
use log::debug;

/// Kubernetes client object
pub struct K8S;

static CLIENT: OnceCell<Client> = OnceCell::new();
static JOBS: OnceCell<Api<Job>> = OnceCell::new();
static PODS: OnceCell<Api<Pod>> = OnceCell::new();

impl K8S {
    /// Create a new Kubernetes client, or return the existing one
    async fn client() -> Client {
        CLIENT
            .get_or_init(async { Client::try_default().await.unwrap() })
            .await
            .clone()
    }
    /// Create a new Kubernetes job API, or return the existing one
    async fn jobs() -> &'static Api<Job> {
        JOBS.get_or_init(async { Api::default_namespaced(K8S::client().await) })
            .await
    }
    /// Create a new Kubernetes pod API, or return the existing one
    async fn pods() -> &'static Api<Pod> {
        PODS.get_or_init(async { Api::default_namespaced(K8S::client().await) })
            .await
    }
}
/// Dispatches build job to the Kubernetes cluster
pub async fn dispatch_build(
    id: String,
    image: String,
    pack_url: String,
    token: String,
    scope: Option<String>,
) -> Result<()> {
    let jobs = K8S::jobs().await;

    // TODO: Issue a build token and pass it into the job

    let mut labels = BTreeMap::new();
    labels.insert("anda-build-id".to_string(), id.clone());

    let mut cmd = vec!["anda".to_string(), "build".to_string()];

    if let Some(scope) = scope {
        cmd.extend(vec!["-p".to_string(), scope])
    }

    cmd.push(pack_url.clone());
    // TODO: add buildkit host here

    let spec = Job {
        metadata: ObjectMeta {
            name: Some(format!("build-{}", id)),
            labels: Some(labels),
            ..ObjectMeta::default()
        },
        spec: Some(JobSpec {
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    restart_policy: Some("Never".to_string()),
                    containers: vec![Container {
                        name: format!("build-container-{}", id),
                        image: Some(image),
                        env: Some(vec![
                            EnvVar {
                                name: "ANDA_BUILD_ID".to_string(),
                                value: Some(id.clone()),
                                ..EnvVar::default()
                            },
                            EnvVar {
                                name: "ANDA_BUILD_TOKEN".to_string(),
                                value: Some(token.clone()),
                                ..EnvVar::default()
                            },
                            EnvVar {
                                name: "ANDA_BUILD_PACK_URL".to_string(),
                                value: Some(pack_url.clone()),
                                ..EnvVar::default()
                            },
                            // replace this with proper host
                            EnvVar {
                                name: "BUILDKIT_HOST".to_string(),
                                value: std::env::var("ANDA_BUILDKIT_HOST").ok(),
                                ..EnvVar::default()
                            },
                            EnvVar {
                                name: "ANDA_ENDPOINT".to_string(),
                                value: std::env::var("ANDA_ENDPOINT").ok(),
                                ..EnvVar::default()
                            },
                        ]),
                        command: Some(cmd),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),

                metadata: Some(ObjectMeta {
                    name: Some(format!("build-pod-{}", id)),
                    ..ObjectMeta::default()
                }),
            },
            backoff_limit: Some(0),
            ..Default::default()
        }),
        ..Default::default()
    };

    let job = jobs.create(&PostParams::default(), &spec).await?;

    //debug!("Created job: {:#?}", job);

    //let logs = watch_jobs().await;
    // stream logs
    let pods = K8S::pods().await;
    let mut stream = pods.watch(&ListParams::default(), "0").await?.boxed();

    let mut watching_pod: Option<Pod> = None;

    /*     while let Some(status) = stream.try_next().await? {
        match status {
            WatchEvent::Added(s) => println!("Added {}", s.name_any()),
            WatchEvent::Modified(s) => println!("Modified: {}", s.name_any()),
            WatchEvent::Deleted(s) => println!("Deleted {}", s.name_any()),
            WatchEvent::Bookmark(s) => {},
            WatchEvent::Error(s) => println!("{}", s),
        }
    } */

    while watching_pod.is_none() {
        if let Some(WatchEvent::Added(pod)) = stream.try_next().await? {
            // check if watchevent is WatchEvent::Added
            watching_pod = Some(pod);
        }
    }

    // wait for pod to be ready
    //let p_stat = watching_pod.clone().unwrap().status.unwrap().container_statuses;
    while let Some(bool) = watching_pod
        .clone()
        .unwrap()
        .status
        .unwrap()
        .container_statuses
    {
        println!("{:?}", bool);

        if bool[0].ready {
            break;
        }
        if let Some(WatchEvent::Modified(pod)) = stream.try_next().await? {
            // check if watchevent is WatchEvent::Added
            watching_pod = Some(pod);
        }
    }

    let pod_name = watching_pod.unwrap().name_any();

    let mut logstream = get_logs(pod_name.clone()).await?.boxed();

    while let Some(log) = logstream.try_next().await? {
        // stream bytes
        let log = String::from_utf8((&log).to_vec())?.to_string();
        debug!("{}", log);
    }

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

pub async fn get_logs(
    id: String,
) -> Result<impl Stream<Item = Result<Bytes, kube::Error>>, kube::Error> {
    let _jobs = K8S::jobs().await;
    let pods = K8S::pods().await;

    // let job = jobs.get(format!("build-{}", id).as_str()).await?;

    // if let Some(spec) = job.spec {
    // if let Some(template) = spec.template {
    // template.
    // }

    // }

    let filter = format!("job-name=build-{}", id);
    println!("filter: {}", filter);

    //let pod = p.items.first().unwrap();
    //debug!("{:#?}", pod);

    //let pod_name = pod.metadata.name.clone().unwrap();
    //todo!();
    pods.log_stream(
        &id,
        &LogParams {
            follow: true,
            pretty: true,
            ..Default::default()
        },
    )
    .await
}
