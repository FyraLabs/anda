import { AppProps } from "next/app";
import "../styles/globals.scss";
import { createTheme, NextUIProvider } from "@nextui-org/react";
import { ThemeProvider as NextThemesProvider } from "next-themes";
import { LogtoProvider, LogtoConfig } from "@logto/react";

const config: LogtoConfig = {
  endpoint: "https://accounts.fyralabs.com",
  appId: "by2Xk45J3sx0zI2tijr0Y",
};

const lightTheme = createTheme({
  type: "light",
});

const darkTheme = createTheme({
  type: "dark",
});

const MyApp = ({ Component, pageProps }: AppProps) => (
  <LogtoProvider config={config}>
    <NextThemesProvider
      defaultTheme="system"
      attribute="class"
      value={{
        light: lightTheme.className,
        dark: darkTheme.className,
      }}
    >
      <NextUIProvider>
        <Component {...pageProps} />
      </NextUIProvider>
    </NextThemesProvider>
  </LogtoProvider>
);

export default MyApp;
