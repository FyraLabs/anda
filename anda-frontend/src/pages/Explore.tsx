import { faStar } from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { getAllProjects } from "../api/projects";
import { Link } from "@tanstack/react-location";

const Explore = () => {
  const query = useQuery(["projects"], getAllProjects);

  if (!query.data) return <></>;

  return (
    <div className="p-5 dark:text-gray-300">
      <h1 className="text-3xl font-bold mb-2">Explore</h1>
      <div className="flex divide-y-[1px] divide-neutral-700 flex-col">
        {query.data.map((project) => (
          <Link to={`/app/projects/${project.id}`} key={project.id}>
            <div className="flex gap-5 items-center py-2 h-14" key={project.id}>
              <div className="flex flex-col">
                <p>{project.name}</p>
                {project.description && (
                  <p className="text-xs font-light">{project.description}</p>
                )}
              </div>
              {/* <div className="flex items-center gap-2 ml-auto">
          120
          <FontAwesomeIcon icon={faStar} />
        </div> */}
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
};

export default Explore;
