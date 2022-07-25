import { useLogto } from "@logto/react";
import { Link } from "@tanstack/react-location";

const Navbar = () => {
  const a = useLogto();

  return (
    <div className="px-5 py-3 bg-blue-800 h-16 flex shadow-md">
      <div className="flex-1 flex justify-center">
        <div className="mr-auto flex gap-5 text-gray-300 text-sm items-center font-medium">
          <img src="/anda.svg" alt="Anda Logo" className="w-10 h-10" />
          <Link>Home</Link>
          <Link>Explore</Link>
        </div>
      </div>
      <div className="flex-1 flex justify-center">
        <input
          type="text"
          className="flex-1 rounded-lg appearance-none bg-white bg-opacity-10 text-white max-w-2xl px-5 placeholder-gray-300 text-sm placeholder-opacity-50 placeholder:font-light"
          placeholder="Enter a command..."
        />
      </div>
      <div className="flex-1 flex justify-center">
        <div className="rounded-full bg-teal-500 h-10 w-10 ml-auto justify-self-end"></div>
      </div>
    </div>
  );
};

export default Navbar;
