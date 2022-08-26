import { faDocker } from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { getArtifactsOfProject, getProject } from "../../api/projects";
import { Artifact } from "../../api/artifacts";
import { Link, useMatch } from "@tanstack/react-location";
import { ArtifactEntry } from "../../components/ArtifactEntry";
import { Skeleton } from "../../components/Skeleton";
import { getBuild } from "../../api/builds";

const AboutBuild = () => {
    const {
        params: { buildID },
      } = useMatch();
    
      const query = useQuery(["builds", buildID], ({ queryKey }) =>
        getBuild(queryKey[1])
      );
  
    if (!query.data) return <Skeleton/>;
  
    return (
      <>
        <p className="text-3xl font-bold mb-3 dark:text-zinc-200">About</p>
  
        <div className="prose text-white">
            {query.data.build_type}
        </div>
      </>
    );
  };
  
export default AboutBuild;
  