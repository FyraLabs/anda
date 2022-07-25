import {
  Router,
  ReactLocation,
  Route,
  DefaultGenerics,
} from "@tanstack/react-location";
import { useDarkMode } from "usehooks-ts";
import Landing from "./pages/Landing";
import { LogtoProvider, LogtoConfig } from "@logto/react";
import AuthCallback from "./pages/AuthCallback";
import Home from "./pages/Home";
import Project from "./pages/Project";

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
    children: [
      {
        path: "/home",
        element: <Home />,
      },
      {
        path: "/project",
        element: <Project />,
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
