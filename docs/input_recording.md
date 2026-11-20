# Local input recording

Set `BC_INPUT_LOG` to a local log filename before starting the game. The title
becomes `Beaver Colony - input recording` when the file opens successfully.
Recording is disabled by default and requires launching the new build.

The log contains session-relative timestamps, keys and button presses/releases,
click coordinates, wheel events, pointer positions sampled at most ten times
per second, UI/menu actions, feedback and simulation command results (including
rejections). `AFTER_FRAME` describes the selection, tool, hovered cell, clock
and UI hit state after that frame's update; it is not pre-click state. Events
from the keyboard and mouse are recorded only while the game window has focus.

`BC_RECORD` separately enables the existing accepted-command replay file.
Neither file can recover raw inputs from a session that was not recording.

The prepared local launcher is
`D:/harness_work/workspaces/beaver/tmp/input-trace/launch.ps1`.
It opens a separate recording executable without stopping another game window.
Its files are `live-inputs.log` and `accepted-commands.txt` in that same folder.

Validation: the workspace all-target check and recording release build passed.
The command-module tests passed with tracing enabled; the validation log
contains successful assignment/cancellation and the rejected out-of-map move.
