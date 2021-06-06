# To-do's list

- [ ] Functionality.
    - [ ] Add existed function "send package" as an option.
    - [ ] Change switch structures for mapping.
    - [x] Request multiple times in the same execution.
        - [ ] It crashes when continues calls happen in short time.
    - [x] Add the device information's option.
    - [ ] Added verbose mode.
    - [ ] How to handle different firmware versions?
    - [x] Set up the CMakeLists with installation.

- [ ] Portability.
    - [x] Create Docker image and container.
        - [x] Flag for development.
        - [x] Flag for production.
    - [ ] Add support for Ubuntu on Docker files.

- [ ] QA - Quality assurance.
    - [ ] Unit test: Create unit test for every file, and function.
    - [ ] Scanners.
        - [ ] Code coverage scanner.
        - [x] Code quality scanner.
        - [ ] Security scanner.
            - [x] SAST.
            - [ ] DAST.
            - [ ] IAST.
    - [ ] Fix alerts.
        - [ ] Code coverage scanner.
        - [x] Code quality scanner.
        - [ ] Security scanner.
            - [x] SAST.
            - [ ] DAST.
            - [ ] IAST.

- [ ] CI/CD - Make similar to GitLab.
    - [x] Stage for Build.
    - [x] Stage for QA.
    - [ ] Stage for Deploy.
    - [ ] Stage for Production.
        - [ ] Release to AUR Arch Linux.
        - [ ] Release to Debian or others.

- [ ] Original list.
    - [ ] Implement “getters” for current headphone state.
        - [ ] Current status of all setters currently implemented.
        - [ ] Date of manufacturing.
    - [ ] Get/set volume.
    - [ ] Port to macOS (and maybe Windows).
    - [ ] Firmware updates?
