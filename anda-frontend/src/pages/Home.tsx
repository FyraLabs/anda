// https://tailwindui.com/components/application-ui/page-examples/home-screens#component-d222be82dfec1951191abe8624a2a1ed

import { faStar } from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";

const Home = () => {
  return (
    <div className="dark:text-gray-300 p-5">
      {/* <h1 className="font-medium text">Good Morning, lleyton</h1> */}
      {/* <h2>Here is a summary of activity across Andaman</h2> */}

      <div className="flex flex-row">
        <div className="flex-1 mr-10">
          <p className="font-bold text-3xl mb-2">Recent</p>

          <div className="flex divide-y-[1px] divide-neutral-700 flex-col">
            <div className="flex gap-5 items-center py-2">
              <div className="flex flex-col">
                <p>neko</p>
                <p className="text-xs font-light">
                  Neko is a project about shipping catgirls in software packages
                </p>
              </div>
              <div className="flex items-center gap-2 ml-auto">
                120
                <FontAwesomeIcon icon={faStar} />
              </div>
            </div>
            <div className="flex gap-5 items-center py-2">
              <div className="flex flex-col">
                <p>pisscord</p>
                <p className="text-xs font-light">A better Discord client</p>
              </div>
              <div className="flex items-center gap-2 ml-auto">
                55
                <FontAwesomeIcon icon={faStar} />
              </div>
            </div>

            <div className="flex gap-5 items-center py-2">
              <div className="flex flex-col">
                <p>testing</p>
                <p className="text-xs font-light">A stupid testing project</p>
              </div>
              <div className="flex items-center gap-2 ml-auto">
                2<FontAwesomeIcon icon={faStar} />
              </div>
            </div>
          </div>
        </div>

        <div className="max-w-xs">
          <p className="font-medium text-xl mb-3">Activity</p>

          <div className="flex divide-y-[1px] divide-neutral-700 flex-col">
            <div className="flex pb-3">
              <div className="rounded-full bg-teal-500 h-10 w-10 self-center mr-3 aspect-square" />

              <div>
                <p className="font-medium">You</p>

                <p className="text-xs font-light">
                  You pushed f92ff2 to lleyton/neko, causing a new compose.
                </p>
              </div>

              <p className="ml-auto text-gray-400">1h</p>
            </div>
            <div className="flex py-3">
              <div className="rounded-full bg-teal-500 h-10 w-10 self-center mr-3 aspect-square" />

              <div>
                <p className="font-medium">You</p>

                <p className="text-xs font-light">
                  You pushed f92ff2 to lleyton/neko, causing a new compose.
                </p>
              </div>

              <p className="ml-auto text-gray-400">1h</p>
            </div>

            <div className="flex pt-3">
              <div className="rounded-full bg-teal-500 h-10 w-10 self-center mr-3 aspect-square" />

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
    </div>
  );
};

export default Home;
