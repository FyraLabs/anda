import { faDocker } from "@fortawesome/free-brands-svg-icons";
import {
  faBox,
  faArrowDown,
  faFileZipper,
  faStar,
} from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";

const User = () => {
  return (
    <div className="p-5">
      <div className="flex gap-3 items-center">
        <div className="rounded-full bg-teal-500 h-10 w-10"></div>
        <h1 className="text-3xl font-medium text-gray-200">lleyton</h1>

        <div></div>
      </div>

      <div className="flex divide-y-[1px] divide-neutral-800 flex-col dark:text-gray-300">
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
