//go:build linux

package main

import "tinygo.org/x/bluetooth"

func writeCharacteristic(char bluetooth.DeviceCharacteristic, data []byte) (int, error) {
	return char.WriteWithoutResponse(data)
}

