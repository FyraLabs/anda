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

const Project = () => {
  return (
    <div className="flex flex-col dark:text-gray-300 flex-1">
      <div className="flex h-full flex-1 items-stretch">
        <div className="p-5 flex flex-col light:bg-neutral-100 dark:bg-neutral-900 w-72 gap-2">
          <p className="text-xl text-gray-400 font-medium">
            <Link to="..">lleyton</Link> /{" "}
            <span className="text-white">neko</span>
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
          <Outlet />
          {/* <p className="text-3xl font-medium mb-3 flex items-center gap-3">
            <span className="flex h-[12px] w-[12px] relative">
              <span className="relative inline-flex rounded-full h-[12px] w-[12px] bg-green-500"></span>
            </span>
            Compose #1
          </p>

          <div className="flex flex-row">
            <p>Logs</p>
            <p>Artifacts</p>
          </div> */}
        </div>
      </div>
    </div>
  );
};

export default Project;
