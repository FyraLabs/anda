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
import About from "./pages/Project/About";
import Composes from "./pages/Project/Composes";
import Artifacts from "./pages/Project/Artifacts";
import User from "./pages/User";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import Explore from "./pages/Explore";
import { getAllProjects, getProject } from "./api/projects";
import { getAllBuilds } from "./api/builds";
import Builds from "./pages/Builds";

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
            element: <About />,
          },
          {
            path: "/about",
            element: <About />,
          },
          {
            path: "/composes",
            element: <Composes />,
          },
          {
            path: "/artifacts",
            element: <Artifacts />,
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
  const darkMode = useDarkMode(true);

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
