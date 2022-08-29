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
import { useEffect, useState, useRef } from "react";
import { APIUrl } from "../../api/client";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Terminal } from "xterm";
import { FitAddon } from "xterm-addon-fit";

const AboutLogs = () => {
  const {
    params: { buildID },
  } = useMatch();
  const parentRef = useRef<HTMLDivElement>(null);

  const [xterm, setXterm] = useState<Terminal | null>();

  useEffect(() => {
    if (!parentRef.current) return;
    const term = new Terminal();
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.options = {
      scrollback: Infinity,
    }
    setXterm(term);
    term.open(parentRef.current);
    fit.fit();

    return () => {
      term.dispose();
      setXterm(null);
    };
  }, [parentRef.current]);

  useEffect(() => {
    if (!xterm) return;
    const eventSource = new EventSource(APIUrl + `/builds/${buildID}/log`);

    eventSource.onmessage = (message) => {
      xterm.writeln(message.data);
    };
    //eventSource.close();

    eventSource.addEventListener("end", () => {
      eventSource.close();
    })
    // now it actually stops looping logs
    return () => {
      eventSource.close();
    };
  }, [buildID, xterm]);

  return (
    <>
      <div className="flex-1" ref={parentRef}></div>
    </>
  );
};

export default AboutLogs;
