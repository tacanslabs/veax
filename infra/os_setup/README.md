### Goal
This is the base directory for scripts that set up the local OS to meet the requirements of this repo.
*These scripts must be written in a way that works both for CI and for a local dev OS*.
They are expected to be idempotent. The scripts should be safe to run in an OS instance, 
shared between many different projects (e.g. CI), and not break anything to anybody.

### Convention
There must be exactly one script that does the job for a given OS, and it's name must indicate 
which OS is that. For example, it may be `darwin.sh` or `ubuntu.sh`. We might have a situation when a script 
is suitable for all linuxes, in which case it's name must be something like `linux.sh`, and there should be no 
`ubuntu.sh` or `arch.sh`; it may be that a script works for both darwin and linuxes, in which case it should be named 
something like `unix.sh`. There may be helper scripts, which may be freely put in `common` subdirectory and reused 
in the final OS-specific scripts.