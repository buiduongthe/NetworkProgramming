// Bài 7.8 – Client MQTT 5.0 tối giản trên socket TCP (cài đặt các lệnh mức thư viện của Mã giả 7.9).
//
// Hỗ trợ: CONNECT (kèm di chúc – Last Will) / CONNACK, PUBLISH QoS 0 và QoS 1 (PUBACK), SUBSCRIBE / SUBACK,
// PINGREQ / PINGRESP, DISCONNECT. Không hỗ trợ QoS 2, TLS, thuộc tính MQTT 5.0 (luôn gửi độ dài thuộc tính 0).
// Định khung MQTT: 1 byte loại gói + cờ, "độ dài còn lại" mã hóa varint 1–4 byte, rồi đúng chừng ấy byte
// (kỹ thuật tiền tố độ dài của Mục 3.4, đọc bằng RECV_EXACT – Mã giả 3.4). Đặc tả: OASIS MQTT 5.0.
package main

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	CONNECT, CONNACK, PUBLISH, PUBACK                = 1, 2, 3, 4
	SUBSCRIBE, SUBACK, PINGREQ, PINGRESP, DISCONNECT = 8, 9, 12, 13, 14
)

// ConnError: lỗi kết nối với broker (broker đóng kết nối, CONNACK/SUBACK từ chối…).
type ConnError struct{ msg string }

func (e *ConnError) Error() string { return e.msg }

// mqttStr: chuỗi UTF-8 của MQTT – 2 byte độ dài + dữ liệu.
func mqttStr(s string) []byte {
	return append(binary.BigEndian.AppendUint16(nil, uint16(len(s))), s...)
}

// varint: 7 bit mỗi byte, bit cao = còn byte tiếp.
func varint(n int) []byte {
	var out []byte
	for {
		d := byte(n & 0x7F)
		n >>= 7
		if n > 0 {
			d |= 0x80
		}
		out = append(out, d)
		if n == 0 {
			return out
		}
	}
}

// topicMatches – bộ lọc chủ đề: '+' khớp đúng một cấp, '#' khớp mọi cấp còn lại (Mục 7.5.5).
func topicMatches(flt, topic string) bool {
	f, t := strings.Split(flt, "/"), strings.Split(topic, "/")
	for i, part := range f {
		if part == "#" {
			return true
		}
		if i >= len(t) || (part != "+" && part != t[i]) {
			return false
		}
	}
	return len(f) == len(t)
}

type handler struct {
	flt string
	fn  func(topic, payload string, qos int, retain bool)
}

// MqttClient: một kết nối tới broker.
type MqttClient struct {
	sock      net.Conn
	nextID    uint16
	handlers  []handler
	keepAlive int
	lastSend  time.Time
	sendMu    sync.Mutex // mỗi lúc gửi một gói (Ctrl+C gửi DISCONNECT từ goroutine khác)
}

func (c *MqttClient) send(ptype, flags byte, body []byte) error {
	c.sendMu.Lock()
	defer c.sendMu.Unlock()
	pkt := append([]byte{ptype<<4 | flags}, varint(len(body))...)
	_, err := c.sock.Write(append(pkt, body...)) // SEND_ALL
	c.lastSend = time.Now()                      // keep alive tính theo gói client GỬI đi
	return err
}

func (c *MqttClient) recvExact(n int) ([]byte, error) {
	buf := make([]byte, n)
	if _, err := io.ReadFull(c.sock, buf); err != nil { // RECV_EXACT
		if ne, ok := err.(net.Error); ok && ne.Timeout() {
			return nil, err
		}
		return nil, &ConnError{"Broker đóng kết nối"}
	}
	return buf, nil
}

// readPacket: đọc một gói; chỉ chờ byte đầu với firstTimeout, phần còn lại chờ tối đa 5 giây.
func (c *MqttClient) readPacket(firstTimeout time.Duration) (byte, byte, []byte, error) {
	c.sock.SetReadDeadline(time.Now().Add(firstTimeout))
	b0, err := c.recvExact(1) // hết giờ ở đây: chưa mất byte nào của gói
	if err != nil {
		return 0, 0, nil, err
	}
	c.sock.SetReadDeadline(time.Now().Add(5 * time.Second))
	n, shift := 0, 0
	for { // giải mã varint "độ dài còn lại"
		d, err := c.recvExact(1)
		if err != nil {
			return 0, 0, nil, err
		}
		n |= int(d[0]&0x7F) << shift
		shift += 7
		if d[0]&0x80 == 0 {
			break
		}
	}
	body, err := c.recvExact(n)
	return b0[0] >> 4, b0[0] & 0x0F, body, err
}

// dispatch: gói PUBLISH đến – tách chủ đề, mã gói (QoS > 0), bỏ thuộc tính, gọi hàm xử lý khớp bộ lọc.
func (c *MqttClient) dispatch(flags byte, body []byte) {
	qos, retain := int(flags>>1)&3, flags&1 == 1
	tlen := int(binary.BigEndian.Uint16(body))
	topic, i := string(body[2:2+tlen]), 2+tlen
	if qos > 0 {
		pid := body[i : i+2]
		i += 2
		c.send(PUBACK, 0, pid) // QoS 1: xác nhận cho broker
	}
	plen, shift := 0, 0
	for { // độ dài thuộc tính (varint) rồi bỏ qua thuộc tính
		d := body[i]
		i++
		plen |= int(d&0x7F) << shift
		shift += 7
		if d&0x80 == 0 {
			break
		}
	}
	payload := string(body[i+plen:])
	for _, h := range c.handlers {
		if topicMatches(h.flt, topic) {
			h.fn(topic, payload, qos, retain)
		}
	}
}

// wait: chờ gói loại ptype; gói PUBLISH đến trong lúc chờ vẫn được xử lý.
func (c *MqttClient) wait(ptype byte) ([]byte, error) {
	for {
		t, flags, body, err := c.readPacket(5 * time.Second)
		if err != nil {
			return nil, err
		}
		if t == ptype {
			return body, nil
		}
		if t == PUBLISH {
			c.dispatch(flags, body)
		}
	}
}

// Will: di chúc (chủ đề, nội dung, qos, retain).
type Will struct {
	topic, msg string
	qos        int
	retain     bool
}

// connect – MQTT_CONNECT(host, port, client_id, keep_alive, will).
func (c *MqttClient) connect(host string, port int, clientID string, keepAlive int, will *Will) error {
	s, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 5*time.Second)
	if err != nil {
		return err
	}
	c.sock, c.keepAlive, c.nextID = s, keepAlive, 1
	flags := byte(0x02) // Clean Start
	payload := mqttStr(clientID)
	if will != nil {
		flags |= 0x04 | byte(will.qos)<<3
		if will.retain {
			flags |= 0x20
		}
		payload = append(payload, varint(0)...) // thuộc tính di chúc (rỗng)
		payload = append(payload, mqttStr(will.topic)...)
		payload = append(payload, mqttStr(will.msg)...)
	}
	body := append(mqttStr("MQTT"), 5, flags) // tên giao thức, mức giao thức 5, cờ kết nối
	body = binary.BigEndian.AppendUint16(body, uint16(keepAlive))
	body = append(body, varint(0)...) // thuộc tính CONNECT (rỗng)
	body = append(body, payload...)
	if err := c.send(CONNECT, 0, body); err != nil {
		return err
	}
	ack, err := c.wait(CONNACK)
	if err != nil {
		return err
	}
	if len(ack) < 2 || ack[1] != 0 { // mã lý do 0x00: thành công
		return &ConnError{fmt.Sprintf("CONNACK mã lý do 0x%02X", ack[1])}
	}
	return nil
}

// publish – PUBLISH(c, topic, payload, qos, retain).
func (c *MqttClient) publish(topic, payload string, qos int, retain bool) error {
	body := mqttStr(topic)
	if qos > 0 {
		body = binary.BigEndian.AppendUint16(body, c.nextID)
		c.nextID++
	}
	flags := byte(qos << 1)
	if retain {
		flags |= 1
	}
	body = append(append(body, varint(0)...), payload...)
	if err := c.send(PUBLISH, flags, body); err != nil {
		return err
	}
	if qos == 1 {
		_, err := c.wait(PUBACK) // QoS 1: chờ broker xác nhận
		return err
	}
	return nil
}

// subscribe – SUBSCRIBE(c, filter, qos, handler); trả về QoS được cấp.
func (c *MqttClient) subscribe(flt string, qos int, fn func(string, string, int, bool)) (int, error) {
	c.handlers = append(c.handlers, handler{flt, fn}) // đăng ký trước: tin giữ lại có thể đến ngay
	body := binary.BigEndian.AppendUint16(nil, c.nextID)
	c.nextID++
	body = append(body, varint(0)...)
	body = append(body, mqttStr(flt)...)
	body = append(body, byte(qos))
	if err := c.send(SUBSCRIBE, 0x2, body); err != nil {
		return 0, err
	}
	ack, err := c.wait(SUBACK)
	if err != nil {
		return 0, err
	}
	code := int(ack[len(ack)-1]) // sau mã gói và thuộc tính (thường rỗng): mã lý do
	if len(ack) > 3 {
		code = int(ack[3+int(ack[2])])
	}
	if code >= 0x80 {
		return code, &ConnError{fmt.Sprintf("SUBACK từ chối %s: 0x%02X", flt, code)}
	}
	return code, nil
}

// runEventLoop – RUN_EVENT_LOOP: nhận gói, gọi hàm xử lý; chưa gửi gì trong keep_alive/2 giây thì gửi PINGREQ.
func (c *MqttClient) runEventLoop(onPing func()) error {
	for {
		c.sendMu.Lock()
		wait := time.Duration(c.keepAlive)*time.Second/2 - time.Since(c.lastSend)
		c.sendMu.Unlock()
		if wait <= 0 {
			if err := c.send(PINGREQ, 0, nil); err != nil {
				return err
			}
			continue
		}
		t, flags, body, err := c.readPacket(wait)
		if ne, ok := err.(net.Error); ok && ne.Timeout() {
			continue
		}
		if err != nil {
			return err
		}
		if t == PUBLISH {
			c.dispatch(flags, body)
		} else if t == PINGRESP && onPing != nil {
			onPing()
		}
	}
}

// disconnect: gửi DISCONNECT mã lý do 0 (đóng bình thường, broker bỏ di chúc) rồi đóng socket.
func (c *MqttClient) disconnect() {
	c.send(DISCONNECT, 0, nil)
	c.sock.Close()
}
