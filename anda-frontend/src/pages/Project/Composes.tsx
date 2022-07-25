const Composes = () => {
  return (
    <>
      <p className="text-3xl font-bold mb-3 text-gray-200">Composes</p>

      <div className="flex divide-y-[1px] divide-neutral-800 flex-col">
        <div className="flex gap-5 items-center py-2">
          <span className="flex h-3 w-3 relative">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-sky-400 opacity-75"></span>
            <span className="relative inline-flex rounded-full h-3 w-3 bg-sky-500"></span>
          </span>
          <div className="flex flex-col">
            <p>Compose #3</p>
            <p className="text-xs font-light">
              <span className="text-gray-400">Triggered by</span>{" "}
              <code>023f37</code> on main
            </p>
          </div>
          <p className="ml-auto text-gray-400">1h</p>
        </div>

        <div className="flex gap-5 items-center py-2">
          <span className="flex h-3 w-3 relative">
            <span className="relative inline-flex rounded-full h-3 w-3 bg-red-500"></span>
          </span>
          <div className="flex flex-col">
            <p>Compose #2</p>
            <p className="text-xs font-light">
              <span className="text-gray-400">Triggered by</span> lleyton
              through web UI
            </p>
          </div>
          <p className="ml-auto text-gray-400">1h</p>
        </div>

        <div className="flex gap-5 items-center py-2">
          <span className="flex h-3 w-3 relative">
            <span className="relative inline-flex rounded-full h-3 w-3 bg-green-500"></span>
          </span>
          <div className="flex flex-col">
            <p>Compose #1</p>
            <p className="text-xs font-light">
              <span className="text-gray-400">Triggered by</span>{" "}
              <code>9273f9</code> on main
            </p>
          </div>
          <p className="ml-auto text-gray-400">1h</p>
        </div>
      </div>
    </>
  );
};

export default Composes;
