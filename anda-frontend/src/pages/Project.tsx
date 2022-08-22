import { Link, Outlet, useMatch } from "@tanstack/react-location";
import Navbar from "../components/Navbar";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import {
  faArrowDown,
  faBox,
  faBoxesPacking,
  faCodeMerge,
  faFileZipper,
  faInfoCircle,
} from "@fortawesome/free-solid-svg-icons";
import { faDocker } from "@fortawesome/free-brands-svg-icons";
import { useQuery } from "@tanstack/react-query";
import { getProject } from "../api/projects";

const Project = () => {
  const {
    params: { projectID },
  } = useMatch();
  const query = useQuery(["projects", projectID], ({ queryKey }) =>
    getProject(queryKey[1])
  );

  if (!query.data) return <></>;

  return (
    <div className="flex flex-col dark:text-gray-300 flex-1">
      <div className="flex h-full flex-1 items-stretch">
        <div className="p-5 flex flex-col light:bg-neutral-100 dark:bg-neutral-900 w-72 gap-2">
          <p className="text-xl text-gray-400 font-medium">
            <span className="dark:text-white text-black">{query.data.name}</span>
          </p>
          <Link className="flex gap-2 items-center rounded h-8" to="about">
            <FontAwesomeIcon icon={faInfoCircle} fixedWidth />
            <p>About</p>
          </Link>
          <Link className="flex gap-2 items-center rounded h-8" to="composes">
            <FontAwesomeIcon icon={faCodeMerge} fixedWidth />
            <p>Composes</p>
          </Link>
          <Link className="flex gap-2 items-center rounded h-8" to="artifacts">
            <FontAwesomeIcon icon={faBoxesPacking} fixedWidth />
            <p>Artifacts</p>
          </Link>
        </div>
        <div className="p-5 dark:text-gray-300 flex-1">
          {/* default outlet: about */}
          <Outlet />
        </div>
      </div>
    </div>
  );
};

export default Project;
