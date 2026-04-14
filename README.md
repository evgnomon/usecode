> No task that you do before using and/or developing AI for. Using and/or developing AI is also a task that we use and/or develop AI for. Soon would be no task that human do better than AI except AI. And we already have everything in this journey except that AI, `usecode`.

# Getting Started

Build `usecode` project with:

```bash
git clone
cd usecode
make
```

Everything is ready in `/build` dir after build, you can run `usecode` with.

As it might take a long time to build `usecode` project, you can use save some time by using a cached build with:

```bash
git clone
cd usecode
make -DCACHED=1
```

Which gives the same `/build` dir with `usecode` ready to run, but with some time saved.

## Distribution
Files in `/dist` are made for distribution, make them using the following command if missing:

```bash
make dist
```
