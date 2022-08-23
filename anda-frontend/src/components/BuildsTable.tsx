import { Link } from "@tanstack/react-location";
import { useQuery } from "@tanstack/react-query";
import { Tooltip } from "flowbite-react";
import { Skeleton } from "./Skeleton";
// import moment.js
import moment from "moment";

import { getAllBuilds } from "../api/builds";
import { getProject } from "../api/projects";

export const BuildsTable = () => {
  const query = useQuery(["builds"], getAllBuilds);
  if (!query.data) return <Skeleton />;
  //console.debug(query.data);
  return (
    <div className="relative shadow-md sm:rounded-xl overflow-y-hidden overflow-x-hidden">
      <table className="w-full text-sm text-left text-gray-500 dark:text-gray-400">
        <thead className="text-xs text-gray-700 uppercase bg-gray-50 dark:bg-zinc-700 dark:text-gray-400">
          <tr>
            <th scope="col" className="py-3 px-6">
              Build
            </th>
            <th scope="col" className="py-3 px-6">
              Build ID
            </th>
            <th scope="col" className="py-3 px-6">
              Target ID
            </th>
            <th scope="col" className="py-3 px-6">
              Project
            </th>
            <th scope="col" className="py-3 px-6">
              Status
            </th>
            <th scope="col" className="py-3 px-6">
              Created
            </th>
          </tr>
        </thead>
        <tbody>
          {query.data.map((build) => (
            <tr className="bg-white border-b dark:bg-zinc-800 dark:border-gray-700">
              <th className="py-3 px-6">{build.build_type}</th>
              <td className="py-3 px-6">{build.id}</td>
              <td className="py-3 px-6">{build.target_id}</td>
              <td className="py-3 px-6">
                {/* if project id is not null */}
                {build.project_id ? (
                  <Link
                    to={`/app/projects/${build.project_id}`}
                    className="dark:text-blue-300 text-blue-500 underline"
                  >
                    {/* Get project name from build id */}
                    {ProjectName(build.project_id)}
                  </Link>
                ) : (
                  "-"
                )}
              </td>
              <td className="py-3 px-6">{StatusBanner(build.status)}</td>
              <td className="py-3 px-6">
                {/* <Tooltip
                content={moment(build.timestamp).format()}
                arrow={false}
                >
                <p>
                  {moment(build.timestamp).fromNow()}
                </p>
                </Tooltip> */}
                <p>
                  {moment(build.timestamp).fromNow()}
                </p>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};

function ProjectName(id: string) {
  const query = useQuery(["project", id], ({ queryKey }) =>
    getProject(queryKey[1])
  );
  if (!query.data) return <></>;
  return query.data.name;
}

const StatusBanner = (status: string) => {
  const stat = status.toUpperCase();
  switch (status.toLowerCase()) {
    // TODO: use the react components for the status banners once its actually usable
    case "pending":
      return (
        <span className="uppercase bg-yellow-100 text-yellow-800 text-xs font-medium mr-2 px-2.5 py-0.5 rounded dark:bg-yellow-200 dark:text-yellow-900">
          {status}
        </span>
      );
    case "success":
      return (
        <span className="uppercase bg-green-100 text-green-800 text-xs font-medium mr-2 px-2.5 py-0.5 rounded dark:bg-green-200 dark:text-green-900">
          {status}
        </span>
      );
    case "running":
      return (
        <span className="uppercase bg-blue-100 text-blue-900 text-xs font-medium mr-2 px-2.5 py-0.5 rounded dark:bg-blue-200 dark:text-blue-800">
          {status}
        </span>
      );
    case "failure":
      return (
        <span className="uppercase bg-red-100 text-red-800 text-xs font-medium mr-2 px-2.5 py-0.5 rounded dark:bg-red-200 dark:text-red-900">
          {status}
        </span>
      );
    default:
      return (
        <span className="uppercase bg-gray-100 text-gray-800 text-xs font-semibold mr-2 px-2.5 py-0.5 rounded dark:bg-gray-700 dark:text-gray-300">
          {status}
        </span>
      );
  }
};
