import { faDocker } from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
  faFile,
  faCompactDisc,
  faFileLines,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { Artifact } from "../api/artifacts";
import { Link, useMatch } from "@tanstack/react-location";

// artifactentry takes an artifact[] as a parameter and returns a div with the artifact information
export const ArtifactEntry = (artifacts: Artifact[]) => {
  return artifacts.map((artifact: Artifact) => (
    <div className="flex gap-5 items-center py-2" key={artifact.id}>
        {iconFromArtifact(artifact)}
      <div className="flex flex-col h-14">
        <p>{artifact.filename}</p>
        <p className="text-xs font-light">
          12MB • <code>v1.2.0</code> • 6h
        </p>

        {artifact.path !== artifact.filename ? (
          <p className="text-xs font-extralight">
            {artifact.path}
          </p>
        ) : (
          <br/>
        )}
      </div>
      <a href={artifact.url} className="ml-auto text-lg">
        <FontAwesomeIcon icon={faArrowDown} className="ml-auto text-lg" />
      </a>
    </div>
  ));
};

function iconFromArtifact(artifact: Artifact) {
  let icon = faFile;

  console.debug(artifact)
  // check from file name
  // variable so stuff can be shorter
  // TODO: Probably a better way to do this,
  // Might require the server to output the mime type
  // which needs some S3 magic
  // also additional metadata embedded with the database
  let name = artifact.filename
  //console.log(name);

  if (
    name.endsWith(".rpm") ||
    name.endsWith(".deb") ||
    name.endsWith(".apk") ||
    name.endsWith(".msi") ||
    name.endsWith(".pkg") ||
    name.endsWith(".dmg")
  ) {
    icon = faBox;
  } else if (
    name.endsWith(".zip") ||
    name.endsWith(".tar") ||
    name.endsWith(".gz") ||
    name.endsWith(".bz2") ||
    name.endsWith(".xz") ||
    name.endsWith(".7z") ||
    name.endsWith(".rar") ||
    name.endsWith(".zst")
  ) {
    icon = faFileZipper;
  } else if (name.endsWith(".iso")) {
    icon = faCompactDisc;
  } else if (
    name.endsWith(".txt") ||
    name.endsWith(".md") ||
    name.endsWith(".log")
  ) {
    icon = faFileLines;
  } else if (
    name.toLowerCase().endsWith("dockerfile") ||
    name.toLowerCase().endsWith("docker-compose.yml") ||
    name.toLowerCase().endsWith("docker-compose.yaml")
  ) {
    icon = faDocker;
  }

  return <FontAwesomeIcon icon={icon} fixedWidth className="text-lg" />;
}
