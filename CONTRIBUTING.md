# Contributing

❤ Contributions of this project are always welcome!

Follow this simple steps to start the adventure:

- [Create a GitHub issue first][new-issue] for new features, changes, bugs,
  doubts, questions or issues.

- Do you want to get your hands dirty? Fork the main branch of the project,
  create a new branch, hack your changes and create a pull request.
    - [Fork a repo][fork-a-repo].
    - [Make a pull request][about-pull-requests].
    - [General information][contribute-github].

- Check the [developer notes][development-notes] to get more familiar with the
  project idea, and structure.

- Optional, we can have a Google meeting to discuss the changes, also we can
  share the screen.

## Versioning and releases

- Version nomenclature `v1.2.20210809.9ee26d8`, this is the description for
  each `vMajorVersion.MinorVersion.Date.CommitID`.
- Each developer works on his own fork.
- Create a merge request directly to `main` branch.
- If this project grows, I'll create the branches `dev` -> `beta` ->
  `main`.

## Contact

It will be a good idea to create a Slack channel or use other platform, but
this project is very small. My personal email is `israel.alberto.rv@gmail.com`.

## Build the application

For developers, it has the same two options for build the code: Docker or
locally.

As a developer, every change needs to complete the quality scanners for that
reason, I recommend using the command `cmake` with the parameters
`AUTOFORMATTING=True` and `VALIDATE_QA=True`. Every pull request that you did
to this repository will run all of these scanners to mark the improvements. For
more information review the script `./src/script/build-dev.bash` and the file
`./src/CMakeLists.txt`.

### Docker

The main advantage to use this is not install many packages and figured out the
compatibility of the versions using for the quality scanners.

Another advantage is mount your local directory to the Docker container which
provide the same files on real-time on both. For example if you don't want to
install locally the package `cppcheck` and figured out about the versions. You
only need to build the binary application in the Docker container and
automatically you will have a build folder in your local machine. The only
package that you need to install in your local machine is the Bluetooth
library: `bluez-libs` or `libbluetooth-dev`.

*Note: If you did some change in the Dockerfile or Compose you need to run
again the commands: `docker-compose down` and
`docker-compose up --detach --build`.*

```bash
# Clean previous docker composes.
# Execute only one time.
docker-compose --project-directory ./src down

# Start the docker compose.
# If you did changes on the Dockerfile you need to run again this command.
# Execute only one time.
export USER_ID=$(id -u ${USER})
export GROUP_ID=$(id -g ${USER})
docker-compose --project-directory ./src up --detach --build

# Make modification to your source.
# After it, you can build the application with your changes.
# Execute at any time.
docker exec \
  --user ${USER_ID} \
  --interactive \
  --tty \
  bose-connect-app-linux \
  /root/bose-connect-app-linux/script/build-dev.bash

# Enjoy.
./src/build/bose-connect-app-linux
```

### Local

Here is more easy, but you need to install in your local machine the next
packages: `make`, `cmake`, `pkgconf`, `clang`, `cppcheck` and `bluez-libs` /
`libbluetooth-dev`.
Also, you need to check the versions, and the compatibility to run the quality
scanners.

```bash
# Make modification to your source.
# After it, you can build the application with your changes.
./src/script/build-dev.bash

# Enjoy.
./build/bose-connect-app-linux
```

[new-issue]: https://github.com/airvzxf/bose-connect-app-linux/issues/new

[fork-a-repo]: https://help.github.com/articles/fork-a-repo/

[about-pull-requests]: https://help.github.com/articles/about-pull-requests/

[contribute-github]: https://docs.github.com/en/github/collaborating-with-pull-requests

[development-notes]: ./DEVELOPMENT.md
