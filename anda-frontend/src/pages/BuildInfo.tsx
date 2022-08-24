import { faDocker } from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { getArtifactsOfProject, getProject } from "../api/projects";
import { Artifact } from "../api/artifacts";
import { Link, useMatch } from "@tanstack/react-location";
import { ArtifactEntry } from "../components/ArtifactEntry";
import { Skeleton } from "../components/Skeleton";
import { getBuild } from "../api/builds";

const BuildInfo = () => {
  const {
    params: { buildID },
  } = useMatch();

  const query = useQuery(["builds", buildID], ({ queryKey }) =>
    getBuild(queryKey[1])
  );

  if (!query.data) return <Skeleton />;

  const project = useQuery(
    ["project", query.data.project_id],
    ({ queryKey }) => {
      if (!queryKey[1]) return;
      return getProject(queryKey[1]);
    }
  );

  return (
    <div className="flex flex-col dark:text-zinc-300">
      <div className="p-14">
        <div className="flex items-center py-6">
          <h1 className="text-2xl font-bold">
            {query.data.build_type} <code>{query.data.id}</code>
          </h1>
        </div>
        <div id="build-id" className="flex items-center py-3">
          <span className="text-zinc-200">
            Build ID: <code>{query.data.id}</code>
          </span>
        </div>

        <div id="build-scratch" className="flex items-center py-3">
          <span className="text-zinc-200">
            Is scratch build: {query.data.project_id ? "yes" : "no"}
          </span>
        </div>
        {project.data ? (
          <div id="build-scratch" className="flex items-center py-3">
            <span className="text-zinc-200">
              For project:{" "}
              <Link to={`/app/projects/${query.data.project_id}`}>
                {project.data.name}
              </Link>
            </span>
          </div>
        ) : null}
      </div>
    </div>
  );
};

export default BuildInfo;
