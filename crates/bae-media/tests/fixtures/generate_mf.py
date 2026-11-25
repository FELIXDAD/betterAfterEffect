"""Generate the tiny multi-GOP MP4s used by Windows decode tests (FFmpeg CLI)."""

import argparse
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--size", default="192x128")
parser.add_argument("--out", type=Path, default=Path(__file__).parent)
args = parser.parse_args()
width, height = map(int, args.size.split("x"))
args.out.mkdir(parents=True, exist_ok=True)
for codec, encoder in [("h264", "libx264"), ("hevc", "libx265"), ("hevc10", "libx265")]:
    options = (
        ["-x264-params", "scenecut=0"]
        if codec == "h264"
        else ["-x265-params", "scenecut=0:pools=2:log-level=error", "-tag:v", "hvc1"]
    )
    pixel_format = "yuv420p10le" if codec == "hevc10" else "yuv420p"
    output = args.out / f"{codec}-{args.size}.mp4"
    raw = output.with_suffix(".source.mp4") if codec == "h264" else output
    audio = ["-f", "lavfi", "-t", "3", "-i", "anullsrc=r=48000:cl=mono",
             "-c:a", "aac"] if codec == "h264" else []
    with subprocess.Popen(
        ["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "gray",
         "-s", args.size, "-r", "30000/1001", "-i", "-", *audio,
         "-c:v", encoder,
         "-vf", f"format={pixel_format},drawbox=x=iw/2:y=0:w=iw/4:h=ih:color=red:t=fill",
         "-pix_fmt", pixel_format,
         "-crf", "12", "-g", "12", "-bf", "2",
         *options, str(raw)],
        stdin=subprocess.PIPE,
    ) as proc:
        for index in range(48):
            top = 24 + index * 41 % 208
            half = width * height // 2
            proc.stdin.write(bytes([top]) * half + bytes([255 - top]) * half)
        proc.stdin.close()
        if proc.wait():
            raise SystemExit(f"{encoder} failed")
    if codec == "h264":
        # Remux the picture to start half a second after the audio.
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-itsoffset", "0.5", "-i", str(raw),
             "-i", str(raw), "-map", "0:v", "-map", "1:a", "-c", "copy", str(output)],
            check=True,
        )
        raw.unlink()
