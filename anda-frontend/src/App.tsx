import {
  Router,
  ReactLocation,
  Route,
  DefaultGenerics,
} from "@tanstack/react-location";
import { createTheme, NextUIProvider } from "@nextui-org/react";
import { useDarkMode } from "usehooks-ts";
import Home from "./pages/Home";
import { LogtoProvider, LogtoConfig } from "@logto/react";
import AuthCallback from "./pages/AuthCallback";

const lightTheme = createTheme({
  type: "light",
});

const darkTheme = createTheme({
  type: "dark",
});

const config: LogtoConfig = {
  endpoint: "https://accounts.fyralabs.com",
  appId: "by2Xk45J3sx0zI2tijr0Y",
};

const location = new ReactLocation();
const routes: Route<DefaultGenerics>[] = [
  {
    path: "/",
    element: <Home />,
  },
  {
    path: "/callback",
    element: <AuthCallback />,
  },
];

const App = () => {
  const darkMode = useDarkMode(false);

  return (
    <LogtoProvider config={config}>
      <NextUIProvider theme={darkMode.isDarkMode ? darkTheme : lightTheme}>
        <Router location={location} routes={routes} />
      </NextUIProvider>
    </LogtoProvider>
  );
};

export default App;
