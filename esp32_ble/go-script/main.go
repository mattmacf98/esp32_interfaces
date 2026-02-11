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

		// Target characteristic UUID (ADC data output)
		// Pin data output: 13c0ef83-09bd-4767-97cb-ee46224ae6db
		// Pin data input (write): c79b2ca7-f39d-4060-8168-816fa26737b7
		// ADC data output: 01037594-1bbb-4490-aa4d-f6d333b42e16
		targetUUID, err := bluetooth.ParseUUID("01037594-1bbb-4490-aa4d-f6d333b42e16")
		if err != nil {
			fmt.Printf("❌ Invalid UUID: %v\n", err)
			os.Exit(1)
		}

		// Discover ALL characteristics (nil = no filter); some stacks don't return
		// all characteristics when filtering by UUID, so we discover all and find by UUID.
		var targetChar bluetooth.DeviceCharacteristic
		found := false

		fmt.Println("🔍 Discovering all characteristics...")
		for _, service := range services {
			chars, err := service.DiscoverCharacteristics(nil)
			if err != nil {
				fmt.Printf("⚠️  DiscoverCharacteristics error for service %s: %v\n", service.UUID().String(), err)
				continue
			}
			fmt.Printf("   Service %s: %d characteristic(s)\n", service.UUID().String(), len(chars))
			for _, c := range chars {
				cu := c.UUID()
				fmt.Printf("      - %s\n", cu.String())
				if cu.String() == targetUUID.String() {
					targetChar = c
					found = true
				}
			}
		}

		if !found {
			fmt.Printf("\n❌ Characteristic %s not found (see list above for what the device exposes)\n", targetUUID.String())
			os.Exit(1)
		}
		fmt.Printf("\n✅ Found target characteristic: %s\n\n", targetUUID.String())

		// ADC DATA OUTPUT
		buffer := make([]byte, 1024)
		readValue, err := targetChar.Read(buffer)
		if err != nil {
			fmt.Printf("❌ Failed to read: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("✅ Read value: %v\n", readValue)
		fmt.Printf("✅ Read value: %v\n", buffer[:readValue])
		numPins := buffer[0]
		for i := 0; i < int(numPins); i++ {
			pin := buffer[i*3+1]
			hsb := buffer[i*3+2]
			lsb := buffer[i*3+3]
			value := (int(hsb) << 8) | int(lsb)
			fmt.Printf("✅ Pin: %d, Value: %d\n", pin, value)
		}

		// REGULAR PIN DATA OUTPUT
		// buffer := make([]byte, 1024)
		// readValue, err := targetChar.Read(buffer)
		// if err != nil {
		// 	fmt.Printf("❌ Failed to read: %v\n", err)
		// 	os.Exit(1)
		// }
		// fmt.Printf("✅ Read value: %v\n", readValue)
		// fmt.Printf("✅ Read value: %v\n", buffer[:readValue])
		// numPins := buffer[0]
		// for i := 0; i < int(numPins); i++ {
		// 	pin := buffer[i*2+1]
		// 	value := buffer[i*2+2]
		// 	fmt.Printf("✅ Pin: %d, Value: %d\n", pin, value)
		// }

		// WRIITNG
		// Write "hello" to the characteristic
		// fmt.Println("✍️  Writing \"hello\" to characteristic...\n")

		// message := []byte("{\"pin_writes\": [{\"pin_num\": 14, \"state\": 100}]}")
		// fmt.Println(len(message))
		// _, err = writeCharacteristic(targetChar, message)
		// if err != nil {
		// 	fmt.Printf("❌ Failed to write: %v\n", err)
		// 	device.Disconnect()
		// 	os.Exit(1)
		// }
		// fmt.Printf("✅ Wrote: \"hello\" (%v)\n", message)
		// fmt.Println("🔌 Disconnecting...")

		// err = device.Disconnect()
		// if err != nil {
		// 	fmt.Printf("⚠️  Disconnect warning: %v\n", err)
		// }

		// fmt.Println("👋 Done!")

	case <-timeout:
		adapter.StopScan()
		fmt.Printf("\n⏱️  Timeout: Device \"%s\" not found after %d seconds\n", *namePtr, *timeoutPtr)
		os.Exit(1)
	}
}
