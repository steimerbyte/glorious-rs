@echo off
ffmpeg -f dshow -video_size 1280x720 -i video="@device_pnp_\\?\usb#vid_05c8&pid_082f&mi_00#6&2e5b859&0&0000#{65e8773d-8f56-11d0-a3b9-00a0c9223196}\global" -frames:v 1 -y cam0.jpg
