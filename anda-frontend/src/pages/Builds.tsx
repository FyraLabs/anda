import { faStar } from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { getAllProjects } from "../api/projects";
import { Link } from "@tanstack/react-location";
import { getAllBuilds } from "../api/builds";
import { BuildsTable } from "../components/BuildsTable";

const Builds = () => {
  return (
    <div className="p-5 dark:text-gray-300">
      <h1 className="text-3xl font-bold mb-2">Builds</h1>
      <BuildsTable/>
    </div>
  );
};

export default Builds;
