// Bài 7.8 – Thiết bị cảm biến phát số đo nhiệt độ qua MQTT (Mã giả 7.9, PROCEDURE SENSOR; Mục 7.5.5).
//
// Kết nối với di chúc (Last Will) "offline" giữ lại, phát "online" (QoS 1, giữ lại), sau đó phát số đo
// JSON {v, ts} QoS 0 mỗi chu kỳ. Khi đủ số lần: tắt đúng thủ tục (phát "offline" rồi DISCONNECT), hoặc
// với tùy chọn --mat-ket-noi thì đóng socket đột ngột để broker tự phát di chúc.
//
// Chạy:  out/app sensor <mã_thiết_bị> [host] [port] [số_lần] [chu_kỳ_ms] [--mat-ket-noi]
//
//	(mặc định localhost 1883, số_lần 0 = chạy mãi, chu kỳ 5000 ms)
package main

import (
	"fmt"
	"math/rand/v2"
	"time"
)

// readSensor – READ_SENSOR(): giá trị mô phỏng 24–32 °C.
func readSensor() float64 {
	return 24 + rand.Float64()*8
}

func sensorMain(all []string) {
	var args []string
	lose := false
	for _, a := range all {
		if a == "--mat-ket-noi" {
			lose = true
		} else {
			args = append(args, a)
		}
	}
	deviceID, host := "day1", "localhost"
	if len(args) >= 1 {
		deviceID = args[0]
	}
	if len(args) >= 2 {
		host = args[1]
	}
	port := portArg(args, 2, 1883)
	times := intArg(args, 3, 0)
	periodMs := intArg(args, 4, 5000)
	st := "nhamay/" + deviceID + "/trangthai"
	c := &MqttClient{}
	err := c.connect(host, port, deviceID, 30, &Will{st, "offline", 1, true}) // MQTT_CONNECT(…, will ← …)
	if err != nil {
		fail(fmt.Sprintf("[%s] Không kết nối được broker: %v", deviceID, err))
	}
	fmt.Printf("[%s] Đã kết nối broker %s:%d (MQTT 5.0, keep alive 30 s, di chúc '%s' = offline)\n", deviceID, host, port, st)
	if err := c.publish(st, "online", 1, true); err != nil {
		fail(fmt.Sprintf("[%s] %v", deviceID, err))
	}
	fmt.Printf("[%s] PUBLISH %s = online (QoS 1, giữ lại) -> đã nhận PUBACK\n", deviceID, st)
	for n := 1; times == 0 || n <= times; n++ {
		msg := fmt.Sprintf(`{"v":%.1f,"ts":%d}`, readSensor(), time.Now().Unix()) // JSON_ENCODE({v, ts: NOW()})
		c.publish("nhamay/"+deviceID+"/nhietdo", msg, 0, false)
		fmt.Printf("[%s] PUBLISH nhamay/%s/nhietdo = %s (QoS 0)\n", deviceID, deviceID, msg)
		time.Sleep(time.Duration(periodMs) * time.Millisecond) // SLEEP(5000)
	}
	if lose {
		c.sock.Close() // mô phỏng mất điện: không gửi DISCONNECT
		fmt.Printf("[%s] Mô phỏng mất kết nối: đóng socket, không gửi DISCONNECT\n", deviceID)
	} else {
		c.publish(st, "offline", 1, true)
		c.disconnect()
		fmt.Printf("[%s] Tắt đúng thủ tục: phát offline rồi gửi DISCONNECT\n", deviceID)
	}
}
