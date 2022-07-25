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

const config: LogtoConfig = {
  endpoint: "https://accounts.fyralabs.com",
  appId: "by2Xk45J3sx0zI2tijr0Y",
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
        path: "/:user",
        children: [
          {
            path: "/",
            element: <User />,
          },
          {
            path: "/:project",
            element: <Project />,
            children: [
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
        ],
      },
    ],
  },
];

const App = () => {
  const darkMode = useDarkMode(true);

  return (
    <LogtoProvider config={config}>
      <Router location={location} routes={routes} />
    </LogtoProvider>
  );
};

export default App;
