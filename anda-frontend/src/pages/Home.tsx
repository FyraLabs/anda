// https://tailwindui.com/components/application-ui/page-examples/home-screens#component-d222be82dfec1951191abe8624a2a1ed

const Home = () => {
  return (
    <div className="dark:text-gray-300 p-5">
      {/* <h1 className="font-medium text">Good Morning, lleyton</h1> */}
      {/* <h2>Here is a summary of activity across Andaman</h2> */}

      <div>
        <p className="font-medium text-2xl mb-3">Activity</p>

        <div className="flex divide-y-[1px] divide-neutral-800 flex-col">
          <div className="flex">
            <div className="rounded-full bg-teal-500 h-10 w-10 self-center mr-3" />

            <div>
              <p className="font-medium">You</p>

              <p className="text-xs font-light">
                You pushed f92ff2 to lleyton/neko, causing a new compose.
              </p>
            </div>

            <p className="ml-auto text-gray-400">1h</p>
          </div>
        </div>
      </div>
    </div>
  );
};

export default Home;
