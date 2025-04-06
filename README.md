# About babypi
Raspberry Pi DIY Baby Monitor

## Features

- Audio/Video monitor: HLS H.264+AAC low latency live stream (iOS / Android / TV / Desktop)
- Baby Telemetry: presence, activity, pose estimation, body temperature
- Notifications: Pushover, Home Assistant, etc.
- Privacy: complete open source solution


# BOM

| Name                  | Qty | Price (as of Mar 2025) | Link   |
|-----------------------|-----|------------------------|--------|
| Raspberry Pi 4b       | 1   | $0 - $100              | [here](https://www.raspberrypi.com/products/raspberry-pi-4-model-b/) |
| IMX219-160 8MP IR-CUT | 1   | $25.99                 | [here](https://www.waveshare.com/imx219-160-camera.htm?sku=18946)    |
| MLX90640 32x24 55 FOV | 1   | $54.99                 | [here](https://www.waveshare.com/MLX90640-D55-Thermal-Camera.htm)    |
| HMMD-mmWave Radar     | 1   | $2.99                  | [here](https://www.waveshare.com/hmmd-mmwave-sensor.htm)             |
| Copper Heatsink       | 1   | $6.94                  | [here](https://www.aliexpress.us/item/3256803627506147.html)         |
| few jumper wires      | 1   | $0 - $1                | [here](https://www.aliexpress.us/item/3256806860151128.html)         |
| m2 nut inserts        | 12  | $0 - $1                | [here](https://www.aliexpress.us/item/3256804856964661.html)         |
| m2 screws             | 12  | $0 - $1                | [here](https://www.amazon.ca/dp/B0D7QCS5FL)                          |
| 1/4-20 x 10mm insert  | 1   | $0 - $1                | [here](https://www.homedepot.ca/product/paulin-1-4-20-x-10-mm-knife-inserts-zinc-plated/1000129443) |
| mono-pod;tri-pod;jig  | 1   | $0 - $50               | any |

and some PLA filament... comes up to $100 - $200 total cost.

## Raspberry Pi

The one in your drawer would probably work just fine. Developed and tested with 4b, after bailing out on improving the thermal management of my 3b. You could go with 3b or 3b+, but make sure to get yourself a big ahh copper heat sink. The project could theoretically work with most Raspberry Pi boards out there, with some careful considerations and/or sacrifices.

## IMX219-160

Honestly you don't really need to get the IR-CUT version. I found that out the hard way. NoIR with IR diode is perfectly fine. Other Raspberry Pi CSI compatible cameras would work just as good (most likely), but having night vision is key for low-light environments like nurseries.

## MLX90640 D55

The reason I went for the narrow FOV version instead of the 110 FOV is because with 110 you'd trade accuracy for unnecessary "real estate". Same sensor, same amount of pixels, more area. That means less information available about a given point in that area.

## HMMD-mmWave Radar

Looking back, I would have probably skipped this and used an accelerometer under the mattress instead. Stay tuned for updates on that...  

## Heatsink

Going for a heatsink is a must. You cant have fans running in your bedroom all the time, that would be crazy. One of the reasons I went for a rpi 4b is because big ahh heatsinks for 4b are more widely available (still) in comparison to 3b or 3b+. You could find some, or at least somewhat compatible ones, but it will cost you more than the board itself costs, so it doesn't really make much sense. The one listed in the BOM keeps the rpi 4b at around ambient+20 when idle and around ambient+30 when under heavy load.

# Assembly

TBD

# Setup

TBD

# Roadmap

- [x] Proof of concept  
- [x] Basic case and assembly  
- [ ] Installation and setup procedure  
- [ ] MVP with secured live stream  
- [ ] MVP with basic UI  
- [ ] Telemetry storage  
- [ ] Audio activity monitoring  
- [ ] Radar activity monitoring  
- [ ] Video presence detection  
- [ ] Video activity monitoring / pose estimation  
- [ ] Body temperature measurement  
- [ ] A/V recording  
- [ ] Notification services  
- [ ] Exhaustive configuration options  
- [ ] Setup and configure utility  

# Notes

TBD

# Acknowledgements

This project sits comfortably on the shoulders of giants. If you wish to give support or contribute to this project, make sure to first give support and contribute the following projects:  
- The [Linux Foundation](https://www.linuxfoundation.org/about/join)
- The [Raspberry Pi Foundation](https://www.raspberrypi.org/donate/)
- The [libcamera](https://libcamera.org/contributing.html) project 
- The [FFmpeg](https://www.ffmpeg.org/donations.html) project
- The [Rust Foundation](https://rustfoundation.org/members/)
- TBC

# License

This project is open sourced under the MIT License.