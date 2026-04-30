const concurrently = require("concurrently");
const path = require("path");

const homeDir = process.env.HOME;

const { result } = concurrently(
  [
    "npm:watch-*", // Run all npm scripts matching watch-*
    {
      command: "nodemon",
      name: "zcore",
      cwd: path.resolve(homeDir, "src/github.com/evgnomon/usecode"), // Working directory relative to $HOME
    },
  ],
  {
    prefix: "name",
    killOthers: ["failure", "success"],
    restartTries: 3,
    cwd: path.resolve(homeDir, "projects"), // Global working directory relative to $HOME
  }
);

result.then(
  () => console.log("All processes completed successfully"),
  (err) => console.error("A process failed:", err)
);
