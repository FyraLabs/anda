use crate::api;
use anyhow::{anyhow, Result};
use clap::{AppSettings, ArgEnum, Parser, Subcommand};
use log::{debug, error, info, trace};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tabwriter::TabWriter;

#[derive(Subcommand)]
pub enum BackendCommand {
    /// List Andaman Artifacts
    ListArtifacts,
    ListBuilds,
}

pub(crate) async fn match_subcmd(cmd: &BackendCommand) -> Result<()> {
    match cmd {
        BackendCommand::ListArtifacts => {
            let artifacts = api::AndaBackend::new(None).list_artifacts().await?;

            let mut writer = TabWriter::new(vec![]);

            writer.write_all(b"ID\tNAME\tBUILD_ID\tTIMESTAMP\n")?;

            for artifact in artifacts {
                writer.write_all(
                    format!(
                        "{}\t{}\t{}\t{}\n",
                        artifact.id.simple(),
                        artifact.name,
                        artifact.build_id.simple(),
                        artifact.timestamp
                    )
                    .as_bytes(),
                )?;
            }
            writer.flush()?;
            let output = String::from_utf8(writer.into_inner()?)?;
            println!("{}", output);
            Ok(())
        }
        BackendCommand::ListBuilds => {
            let builds = api::AndaBackend::new(None).list_builds().await?;

            let mut writer = TabWriter::new(vec![]);

            writer.write_all(b"ID\tWORKER\tSTATUS\tPROJECT_ID\tTIMESTAMP\tCOMPOSE_ID\n")?;

            for build in builds {
                writer.write_all(
                    format!(
                        "{}\t{}\t{}\t{}\t{}\t{}\n",
                        build.id.simple(),
                        build.worker.simple(),
                        build.status,
                        build
                            .project_id
                            .map(|id| id.simple().to_string())
                            .unwrap_or("".to_string()),
                        build.timestamp,
                        build
                            .compose_id
                            .map(|id| id.simple().to_string())
                            .unwrap_or("".to_string()),
                    )
                    .as_bytes(),
                )?;
            }
            writer.flush()?;
            let output = String::from_utf8(writer.into_inner()?)?;
            println!("{}", output);
            Ok(())
        }
    }
}
