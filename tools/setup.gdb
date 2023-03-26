target extended-remote localhost:3333

monitor rtt server start 8765 0
monitor rtt setup 0x20020040 0x30 "SEGGER RTT"
monitor rtt start
monitor rtt setup 0x20020040 0x30 "_SEGGER_RTT"
monitor rtt start
