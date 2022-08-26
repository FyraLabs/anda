import { useMatch } from "@tanstack/react-location";
import { useQuery } from "@tanstack/react-query";
import { getProject } from "../../api/projects";
import ReactMarkdown from 'react-markdown'
import { Skeleton } from "../../components/Skeleton";

const AboutProject = () => {
  const {
    params: { projectID },
  } = useMatch();
  const query = useQuery(["projects", projectID], ({ queryKey }) =>
    getProject(queryKey[1])
  );

  if (!query.data) return <Skeleton/>;

  return (
    <>
      <p className="text-3xl font-bold mb-3 dark:text-zinc-200">About</p>

      <div className="prose text-white">
        <ReactMarkdown className="prose lg:prose-base md:prose-base prose-zinc prose-a:text-blue-600 hover:prose-a:text-blue-500 hover:prose-a:underline dark:text-zinc-200
          dark:prose-headings:text-zinc-300
        ">
        {query.data.description ?? "This project has no description."}
        </ReactMarkdown>
      </div>
    </>
  );
};

export default AboutProject;
