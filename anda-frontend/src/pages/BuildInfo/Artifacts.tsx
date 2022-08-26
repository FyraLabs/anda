import { faDocker } from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useQuery } from "@tanstack/react-query";
import { getArtifactsOfProject } from "../../api/projects";
import { Artifact } from "../../api/artifacts";
import { Link, useMatch } from "@tanstack/react-location";
import { ArtifactEntry } from "../../components/ArtifactEntry";
import { Skeleton } from "../../components/Skeleton";
import { getArtifactsOfBuild, getBuild } from "../../api/builds";

const BuildArtifacts = () => {
    const {
        params: { buildID },
      } = useMatch();
    
      const query = useQuery(["artifacts", buildID], ({ queryKey }) =>
        getArtifactsOfBuild(queryKey[1])
      );
  
    if (!query.data) return <Skeleton/>;
    const artifacts = query.data as Artifact[];
    //console.log(artifacts);
    return (
        <>
        <p className="text-3xl font-bold mb-3 dark:text-zinc-200">Artifacts</p>
  
        <div className="flex divide-y-[1px] divide-neutral-700 flex-col">
          {ArtifactEntry(query.data)}
        </div>
      </>
    );
  };

export default BuildArtifacts;