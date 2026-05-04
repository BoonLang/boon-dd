# Repository Instructions

- When starting any GUI app or command that creates a window from this repo, run it through `cosmic-background-launch --workspace boon-dd -- ...` so the window opens on the `boon-dd` workspace without stealing focus. Keep the wrapper as close as possible to the actual window-creating command so child processes inherit the launch context.
