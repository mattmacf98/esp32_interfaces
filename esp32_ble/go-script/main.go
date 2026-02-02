package main

import (
	"flag"
	"fmt"
	"os"
	"strings"
	"time"

	"tinygo.org/x/bluetooth"
)

var adapter = bluetooth.DefaultAdapter

func main() {
	namePtr := flag.String("name", "", "Name of the Bluetooth device to connect to (required)")
	timeoutPtr := flag.Int("timeout", 30, "Scan timeout in seconds")
	flag.Parse()

	if *namePtr == "" {
		fmt.Println("Error: --name flag is required")
		fmt.Println("\nUsage:")
		flag.PrintDefaults()
		os.Exit(1)
	}

	fmt.Printf("🔍 Scanning for Bluetooth device: \"%s\"\n", *namePtr)
	fmt.Printf("⏱️  Timeout: %d seconds\n\n", *timeoutPtr)

	// Enable the Bluetooth adapter
	err := adapter.Enable()
	if err != nil {
		fmt.Printf("❌ Failed to enable Bluetooth adapter: %v\n", err)
		os.Exit(1)
	}

	// Channel to signal when device is found
	deviceFound := make(chan bluetooth.ScanResult, 1)
	timeout := time.After(time.Duration(*timeoutPtr) * time.Second)

	// Start scanning
	go func() {
		err := adapter.Scan(func(adapter *bluetooth.Adapter, result bluetooth.ScanResult) {
			deviceName := result.LocalName()

			// Print all discovered devices for visibility
			if deviceName != "" {
				fmt.Printf("📱 Found: %s (Address: %s, RSSI: %d dBm)\n",
					deviceName, result.Address.String(), result.RSSI)
			}

			// Check if this is the device we're looking for (case-insensitive)
			if strings.EqualFold(deviceName, *namePtr) {
				select {
				case deviceFound <- result:
					adapter.StopScan()
				default:
				}
			}
		})

		if err != nil {
			fmt.Printf("❌ Scan error: %v\n", err)
			os.Exit(1)
		}
	}()

	// Wait for device to be found or timeout
	select {
	case result := <-deviceFound:
		fmt.Printf("\n✅ Found target device: %s\n", result.LocalName())
		fmt.Printf("📍 Address: %s\n", result.Address.String())
		fmt.Printf("📶 Signal strength: %d dBm\n\n", result.RSSI)

		// Connect to the device
		fmt.Println("🔌 Connecting...")

		device, err := adapter.Connect(result.Address, bluetooth.ConnectionParams{})
		if err != nil {
			fmt.Printf("❌ Failed to connect: %v\n", err)
			os.Exit(1)
		}

		fmt.Printf("✅ Successfully connected to %s!\n", result.LocalName())
		fmt.Printf("🔗 Connection handle: %v\n\n", device)

		// Discover services
		fmt.Println("🔍 Discovering services...")
		services, err := device.DiscoverServices(nil)
		if err != nil {
			fmt.Printf("❌ Failed to discover services: %v\n", err)
			os.Exit(1)
		}

		fmt.Printf("📋 Found %d service(s)\n\n", len(services))

		// Target characteristic UUID
		targetUUID, err := bluetooth.ParseUUID("c79b2ca7-f39d-4060-8168-816fa26737b7")
		if err != nil {
			fmt.Printf("❌ Invalid UUID: %v\n", err)
			os.Exit(1)
		}

		// Find the target characteristic
		var targetChar bluetooth.DeviceCharacteristic
		found := false

		fmt.Println("🔍 Searching for target characteristic...")
		for _, service := range services {
			chars, err := service.DiscoverCharacteristics([]bluetooth.UUID{targetUUID})
			if err != nil {
				continue
			}

			if len(chars) > 0 {
				targetChar = chars[0]
				found = true
				fmt.Printf("✅ Found characteristic: %s\n", targetUUID.String())
				fmt.Printf("📍 In service: %s\n\n", service.UUID().String())
				break
			}
		}

		if !found {
			fmt.Printf("❌ Characteristic %s not found\n", targetUUID.String())
			os.Exit(1)
		}

		// Write "hello" to the characteristic
		fmt.Println("✍️  Writing \"hello\" to characteristic...\n")

		message := []byte("{\"pin_writes\": [{\"pin_num\": 14, \"state\": 0}]}")
		fmt.Println(len(message))
		_, err = targetChar.Write(message)
		if err != nil {
			fmt.Printf("❌ Failed to write: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("✅ Wrote: \"hello\" (%v)\n", message)
		fmt.Println("🔌 Disconnecting...")

		err = device.Disconnect()
		if err != nil {
			fmt.Printf("⚠️  Disconnect warning: %v\n", err)
		}

		fmt.Println("👋 Done!")

	case <-timeout:
		adapter.StopScan()
		fmt.Printf("\n⏱️  Timeout: Device \"%s\" not found after %d seconds\n", *namePtr, *timeoutPtr)
		os.Exit(1)
	}
}
