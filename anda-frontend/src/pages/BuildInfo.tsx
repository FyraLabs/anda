import { faDocker } from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
  faInfoCircle,
  faFileText,
  faBoxesPacking,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { getArtifactsOfProject, getProject } from "../api/projects";
import { Artifact } from "../api/artifacts";
import { Link, Outlet, useMatch } from "@tanstack/react-location";
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
    <div className="flex flex-col dark:text-zinc-300 flex-1">
      <div className="flex h-full flex-1 items-stretch">
        <aside className="p-5 flex flex-col light:bg-neutral-100 dark:bg-zinc-800 w-72 gap-2">
          <p className="text-xl text-gray-400 font-medium">
            <span className="dark:text-white text-black">
              {query.data.build_type}
            </span>
          </p>
          <ul className="space-y-2 list-none">
            <li>
              <Link className="flex gap-2 items-center rounded h-8" to="about">
                <FontAwesomeIcon icon={faInfoCircle} fixedWidth />
                <p>About</p>
              </Link>
            </li>
            <li>
              <Link className="flex gap-2 items-center rounded h-8" to="logs">
                <FontAwesomeIcon icon={faFileText} fixedWidth />
                <p>Logs</p>
              </Link>
            </li>
            <li>
              <Link
                className="flex gap-2 items-center rounded h-8"
                to="artifacts"
              >
                <FontAwesomeIcon icon={faBoxesPacking} fixedWidth />
                <p>Artifacts</p>
              </Link>
            </li>
          </ul>
        </aside>
        <div className="p-5 dark:text-gray-300 flex-1 flex flex-col">
          <Outlet />
        </div>
      </div>
    </div>
  );
};

export default BuildInfo;
