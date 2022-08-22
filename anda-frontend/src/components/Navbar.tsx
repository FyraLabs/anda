import { faMoon, faSun } from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { useLogto } from "@logto/react";
import { Link } from "@tanstack/react-location";
import { useState } from "react";
const Navbar = () => {
  const logto = useLogto();
  const user = logto.getIdTokenClaims();
  const mode = window.matchMedia("(prefers-color-scheme: dark)").matches;

  // get dark mode state from localStorage or use `mode` as default
  const [darkMode, setDarkMode] = useState(localStorage.getItem("color-theme") === "true" || mode);

  if (darkMode) {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
  const toggleDarkMode = () => {
    setDarkMode(!darkMode);
    // check if dark mode is enabled
    if (darkMode) {
      // if dark mode is enabled, set localStorage to false
      localStorage.setItem("color-theme", "light");
      document.documentElement.classList.add("dark");
    }
    else {
      // if dark mode is disabled, set localStorage to true
      localStorage.setItem("color-theme", "dark");
      document.documentElement.classList.remove("dark");
    }
  }

  return (
    <div className="px-5 py-3 bg-zinc-800 h-16 flex shadow-md">
      <div className="flex-1 flex justify-center">
        <div className="mr-auto flex gap-5 text-gray-300 text-sm items-center font-medium">
          <img src="/anda.svg" alt="Andaman Logo" className="w-10 h-10" />
          <Link to="home">Home</Link>
          <Link to="explore">Explore</Link>
          <Link to="builds">Builds</Link>
        </div>
      </div>
      <div className="flex-1 flex justify-center">
        <input
          type="text"
          className="flex-1 rounded-lg appearance-none bg-white bg-opacity-10 text-white max-w-2xl px-5 placeholder-gray-300 text-sm placeholder-opacity-50 placeholder:font-light"
          placeholder="Enter a command..."
        />
      </div>
      {/* dark mode toggle */}

      {user ? (
        <div className="flex-1 flex justify-center">
          <div className="rounded-full bg-teal-500 h-10 w-10 ml-auto justify-self-end"></div>
        </div>
      ) : (
        <div className="flex-1 flex justify-center" />
      )}

      <button
        id="theme-toggle"
        type="button"
        className="text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700 focus:outline-none focus:ring-4 focus:ring-gray-200 dark:focus:ring-gray-700 rounded-lg text-sm p-2.5 justify-self-end"
        onClick={toggleDarkMode}
      >
        <FontAwesomeIcon icon={darkMode ? faMoon : faSun} />
      </button>
    </div>
  );
};

export default Navbar;
