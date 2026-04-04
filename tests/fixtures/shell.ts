import { exec, spawn } from 'child_process';

exec("ffmpeg -i input.mp4 output.webm");
spawn("convert", ["-resize", "100x100"]);
exec("git rev-parse HEAD");
