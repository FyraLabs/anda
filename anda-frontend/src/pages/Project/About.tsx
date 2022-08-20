import { useMatch } from "@tanstack/react-location";
import { useQuery } from "@tanstack/react-query";
import { getProject } from "../../api/projects";

const About = () => {
  const {
    params: { projectID },
  } = useMatch();
  const query = useQuery(["projects", projectID], ({ queryKey }) =>
    getProject(queryKey[1])
  );

  if (!query.data) return <></>;

  return (
    <>
      <p className="text-3xl font-bold mb-3 text-gray-200">About</p>

      <div className="space-y-2">
        {query.data.description ?? "This project has no description."}
      </div>
    </>
  );
};

export default About;
