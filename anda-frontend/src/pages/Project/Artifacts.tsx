import { faDocker } from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { getArtifactsOfProject } from "../../api/projects";
import { Artifact } from "../../api/artifacts";
import { Link, useMatch } from "@tanstack/react-location";
import { ArtifactEntry } from "../../components/ArtifactEntry";
import { Skeleton } from "../../components/Skeleton";

const ProjectArtifacts = () => {
  const {
    params: { projectID },
  } = useMatch();
  const query = useQuery(["artifacts", projectID], ({ queryKey }) =>
    getArtifactsOfProject(queryKey[1])
  );
  if (!query.data) return <Skeleton />;
  const artifacts = query.data as Artifact[];
  //console.log;
  return (
    <>
      <p className="text-3xl font-bold mb-3 dark:text-zinc-200">Artifacts</p>

      <div className="flex divide-y-[1px] divide-neutral-700 flex-col">
        {ArtifactEntry(artifacts)}
      </div>
    </>
  );
};

export default ProjectArtifacts;
