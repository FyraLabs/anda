import {
  faDocker,
  faGithub,
  faTwitter,
} from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
  faStar,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";

const User = () => {
  return (
    <div className="p-5 dark:text-gray-300">
      <div className="flex flex-row items-center gap-5">
        <div className="rounded-full bg-teal-500 h-16 w-16"></div>
        <div className="flex flex-col">
          <h1 className="text-3xl font-bold text-gray-200">lleyton</h1>

          <p>
            Hi! My name is Lleyton Gray. I'm a developer from Los Angeles
            working on packaging for tauOS.
          </p>

          <div className="flex text-sm gap-2">
            <div>
              <FontAwesomeIcon icon={faTwitter} fixedWidth className="mr-1" />
              @lleyton__
            </div>

            <div>
              <FontAwesomeIcon icon={faGithub} fixedWidth className="mr-1" />
              @lleyton
            </div>
          </div>
        </div>
      </div>
      <div className="flex divide-y-[1px] divide-neutral-800 flex-col">
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
  );
};

export default User;
