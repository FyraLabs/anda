import {
  Router,
  ReactLocation,
  Route,
  DefaultGenerics,
  Outlet,
} from "@tanstack/react-location";
import { useDarkMode } from "usehooks-ts";
import Landing from "./pages/Landing";
import { LogtoProvider, LogtoConfig } from "@logto/react";
import AuthCallback from "./pages/AuthCallback";
import Home from "./pages/Home";
import Project from "./pages/Project";
import Navbar from "./components/Navbar";
import AboutProject from "./pages/Project/About";
import ProjectComposes from "./pages/Project/Composes";
import ProjectArtifacts from "./pages/Project/Artifacts";
import User from "./pages/User";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import Explore from "./pages/Explore";
import { getAllProjects, getProject } from "./api/projects";
import { getAllBuilds, getBuild } from "./api/builds";
import Builds from "./pages/Builds";
import { useState } from "react";
import BuildInfo from "./pages/BuildInfo";
import AboutBuild from "./pages/BuildInfo/About";
import BuildArtifacts from "./pages/BuildInfo/Artifacts";
import Logs from "./pages/BuildInfo/Logs";

const config: LogtoConfig = {
  endpoint: "https://accounts.fyralabs.com",
  appId: "Qcg1z97f7oO6Xph0sX4xF",
};

const location = new ReactLocation();
const routes: Route<DefaultGenerics>[] = [
  {
    path: "/",
    element: <Landing />,
  },
  {
    path: "/callback",
    element: <AuthCallback />,
  },
  {
    path: "/app",
    // TODO: is there a better way?
    element: (
      <div className="min-h-screen flex flex-col">
        <Navbar />
        <Outlet />
      </div>
    ),
    children: [
      {
        path: "/home",
        element: <Home />,
      },
      {
        path: "/explore",
        element: <Explore />,
        loader: () =>
          queryClient.getQueryData(["projects"]) ??
          queryClient.fetchQuery(["projects"], getAllProjects),
      },
      {
        path: "/builds",
        element: <Builds />,
        loader: () =>
          queryClient.getQueryData(["builds"]) ??
          queryClient.fetchQuery(["builds"], getAllBuilds),
      },
      {
        path: "/build_info/:buildID",
        element: <BuildInfo />,
        loader: ({ params: { buildID } }) =>
          queryClient.getQueryData(["builds", buildID]) ??
          queryClient.fetchQuery(["builds", buildID], ({ queryKey }) =>
            getBuild(queryKey[1])
          ),
        children: [
          {
            path: "/",
            element: <AboutBuild />,
          },
          {
            path: "/about",
            element: <AboutBuild />,
          },
          {
            path: "/artifacts",
            element: <BuildArtifacts />,
          },
          {
            path: "/logs",
            element: <Logs />,
          },
        ],
      },
      {
        path: "/projects/:projectID",
        element: <Project />,
        loader: ({ params: { projectID } }) =>
          queryClient.getQueryData(["projects", projectID]) ??
          queryClient.fetchQuery(["projects", projectID], ({ queryKey }) =>
            getProject(queryKey[1])
          ),
        children: [
          {
            path: "/",
            element: <AboutProject />,
          },
          {
            path: "/about",
            element: <AboutProject />,
          },
          {
            path: "/composes",
            element: <ProjectComposes />,
          },
          {
            path: "/artifacts",
            element: <ProjectArtifacts />,
          },
        ],
      },
      // {
      //   path: "/:user",
      //   children: [
      //     {
      //       path: "/",
      //       element: <User />,
      //     },
      //     {
      //       path: "/:project",
      //       element: <Project />,
      //       children: [
      //         {
      //           path: "/about",
      //           element: <About />,
      //         },
      //         {
      //           path: "/composes",
      //           element: <Composes />,
      //         },
      //         {
      //           path: "/artifacts",
      //           element: <Artifacts />,
      //         },
      //       ],
      //     },
      //   ],
      // },
    ],
  },
];

const queryClient = new QueryClient();

const App = () => {
  const mode = window.matchMedia("(prefers-color-scheme: dark)").matches;

  // get dark mode state from localStorage or use `mode` as default
  const [darkMode, setDarkMode] = useState(
    localStorage.getItem("color-theme") === "true" || mode
  );

  if (darkMode) {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }

  return (
    <QueryClientProvider client={queryClient}>
      <LogtoProvider config={config}>
        <Router location={location} routes={routes} />
      </LogtoProvider>
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  );
};

export default App;
