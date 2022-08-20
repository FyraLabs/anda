import { useLogto } from "@logto/react";
import { Link } from "@tanstack/react-location";

const Landing = () => {
  const { signIn } = useLogto();

  return (
    <div className="px-4 py-16 mx-auto sm:max-w-xl md:max-w-full lg:max-w-screen-xl md:px-24 lg:px-8 lg:py-20">
      <div className="flex flex-col justify-between lg:flex-row">
        <div className="mb-12 lg:max-w-lg lg:pr-5 lg:mb-0">
          <div className="max-w-xl mb-6">
            <h2 className="max-w-lg mb-6 font-sans text-3xl font-bold tracking-tight text-gray-900 dark:text-gray-100 sm:text-4xl sm:leading-none">
              A modern buildsystem for tauOS and Ultramarine.
            </h2>
            <p className="text-base text-gray-700 dark:text-gray-300 md:text-lg">
              Build, deploy, and collaborate on packages without any friction.
              Open source and 100% free for open source projects.
            </p>
          </div>
          <div className="flex flex-row gap-3">
            <button
              className="rounded-lg bg-blue-600 px-6 py-2.5 text-white font-medium text-xs shadow-md hover:bg-blue-800 transition"
              onClick={() => signIn("http://127.0.0.1:5173/callback")}
            >
              Sign Up
            </button>
            <Link
              className="rounded-lg text-fuchsia-600 dark:text-fuchsia-400 bg-fuchsia-400 px-6 py-2.5 font-medium text-xs shadow-md dark:hover:bg-fuchsia-800 hover:bg-fuchsia-300 transition bg-opacity-40"
              to="/app/home"
            >
              Explore
            </Link>
          </div>
        </div>
        <div className="px-5 pt-6 pb-5">{/* Image here */}</div>
      </div>
    </div>
  );
};

export default Landing;
