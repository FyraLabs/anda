use bytes::Bytes;
use k8s_openapi::api::core::v1::Pod;
use kube::ResourceExt;
use kube::api::{ListParams, LogParams};
use kube::core::WatchEvent;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Rocket, Orbit};
use sea_orm::prelude::Uuid;
use crate::kubernetes::{watch_jobs, BuildStatusEvent, K8S, get_logs, get_full_logs};
use futures::{stream, Stream, StreamExt, TryStreamExt, TryStream};
use tokio::io::AsyncBufReadExt;
use anyhow::Result;
pub struct TaskManager;


#[rocket::async_trait]
impl Fairing for TaskManager {
    fn info(&self) -> Info {
        Info {
            name: "Kubernetes Task Manager",
            kind: Kind::Liftoff
        }
    }

    async fn on_liftoff(&self, _rocket: &Rocket<Orbit>) {
        // Initialize the kubernetes task manager
        watch_status().await.unwrap();
        //let _logs = watch_pods().await;
        watch_logs().await.unwrap();
    }
}


async fn watch_status() -> Result<()> {
    // Initialize the kubernetes task manager
    let mut jobstream = watch_jobs().await.boxed();
    tokio::spawn(async move {
        while let Some(job) = jobstream.next().await {
            match job {
                BuildStatusEvent::Running(id) => {
                    println!("Running: {}", id);
                    // Update build statusd
                    let build = crate::backend::Build::get(Uuid::parse_str(&id).unwrap()).await.unwrap();
                    build.update_status(crate::backend::BuildStatus::Running as i32).await.unwrap();
                    //print_logs(id).await.unwrap();
                }
                BuildStatusEvent::Succeeded(id) => {
                    println!("Succeeded: {}", id);
                    let build = crate::backend::Build::get(Uuid::parse_str(&id).unwrap()).await.unwrap();
                    build.update_status(crate::backend::BuildStatus::Success as i32).await.unwrap();
                    // update logs
                    // let logs = full_logs(id).await.unwrap();
                    // build.update_logs(logs).await.unwrap();

                }
                BuildStatusEvent::Failed(id) => {
                    println!("Failed: {}", id);
                    let build = crate::backend::Build::get(Uuid::parse_str(&id).unwrap()).await.unwrap();
                    build.update_status(crate::backend::BuildStatus::Failure as i32).await.unwrap();
                    // update logs
                    // let logs = full_logs(id).await.unwrap();
                    // build.update_logs(logs).await.unwrap();
                }
            }
        }
    });
    Ok(())
}

async fn watch_logs() -> Result<()> {
    // Initialize the kubernetes task manager
    let mut jobstream = watch_jobs().await.boxed();
    tokio::spawn(async move {
        while let Some(job) = jobstream.next().await {
            match job {
                BuildStatusEvent::Running(_) => {
                    // do nothing
                }
                BuildStatusEvent::Succeeded(id) => {
                    println!("Succeeded: {}", id);
                    let build = crate::backend::Build::get(Uuid::parse_str(&id).unwrap()).await.unwrap();
                    //build.update_status(crate::backend::BuildStatus::Success as i32).await.unwrap();
                    // update logs
                    let logs = full_logs(id).await.unwrap();
                    build.update_logs(logs).await.unwrap();

                }
                BuildStatusEvent::Failed(id) => {
                    println!("Failed: {}", id);
                    let build = crate::backend::Build::get(Uuid::parse_str(&id).unwrap()).await.unwrap();
                    build.update_status(crate::backend::BuildStatus::Failure as i32).await.unwrap();
                    // update logs
                    let logs = full_logs(id).await.unwrap();
                    build.update_logs(logs).await.unwrap();
                }
            }
        }
    });
    Ok(())
}

pub async fn get_pod_name_existing(id: String) -> Result<String> {
    //let pods = K8S::pods().await.clone();
    let client = kube::Client::try_default().await.unwrap();
    let pods: kube::Api<Pod> = kube::Api::default_namespaced(client);
    let filter = format!("job-name=build-{}", id);


    let list = pods.list(&ListParams::default().labels(&filter)).await?;
    
    let pod = list.items.into_iter().next().unwrap();
    //println!("pod: {:#?}", pod);

    Ok(pod.metadata.name.unwrap())
}

pub async fn get_pod_name_watch(id: String) -> Result<String> {
    let filter = format!("job-name=build-{}", id);
    let pods = K8S::pods().await;
    let mut stream = pods.watch(&ListParams::default().labels(&filter), "0").await?.boxed();
    let mut watching_pod: Option<Pod> = None;
    while watching_pod.is_none() {
        if let Some(WatchEvent::Added(pod)) = stream.try_next().await? {
            // check if watchevent is WatchEvent::Added
            watching_pod = Some(pod);
        }
    }
    while let Some(bool) = watching_pod
        .clone()
        .unwrap()
        .status
        .unwrap()
        .container_statuses
    {
        //println!("{:?}", bool);

        if bool[0].ready {
            break;
        }
        if let Some(WatchEvent::Modified(pod)) = stream.try_next().await? {
            // check if watchevent is WatchEvent::Added
            watching_pod = Some(pod);
        }
    }
    let pod_name = watching_pod.unwrap().name_any();
    Ok(pod_name)

}

pub async fn stream_logs(
    id: String,
) -> Result<impl Stream<Item = Result<Bytes, kube::Error>>, kube::Error>{
    let pod_name = get_pod_name_watch(id).await.unwrap();
    get_logs(pod_name.clone()).await
}

pub async fn full_logs(
    id: String,
) -> Result<String> {
    let pod_name = get_pod_name_existing(id).await?;
    Ok(get_full_logs(pod_name.clone()).await?)
}

pub async fn full_logs_db(
    id: String,
) -> Result<String> {
    // get build by id
    let build = crate::backend::Build::get(Uuid::parse_str(&id).unwrap()).await.unwrap();
    Ok(build.logs.unwrap_or_else(|| "".to_string()))
}


async fn print_logs(id: String) -> Result<()> {
    let mut logstream = stream_logs(id).await?;
    let mut real_logstream = format_actual_stream(&mut logstream).await?.boxed();
    while let Some(log) = real_logstream.next().await {
        // stream bytes
        debug!("log: {}", log);
    }
    Ok(())
}


pub async fn format_actual_stream(logstream: impl Stream<Item = Result<Bytes, kube::Error>>) -> Result<impl Stream<Item = String>> {
    // map this logstream into a trystream of strings
    Ok(logstream.map(|bytes| 
        String::from_utf8(bytes.unwrap().to_vec()).expect("Could not convert bytes to string").strip_suffix('\n').unwrap().to_string()
    ))
}

pub async fn format_stream(logs: String) -> Result<impl Stream<Item = String>> {
    // map this logstream into a trystream of strings
    let l: Vec<String> = logs.lines().map(|l| l.to_string()).collect();
    Ok(stream::iter(l))

}