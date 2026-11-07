// Bài mở rộng MR6.15 – MQTT broker tối giản (MQTT 3.1.1 trên TCP), viết tay để thấy rõ giao thức.
//
// Đủ cho bài theo dõi tài xế: tài xế PUBLISH vị trí lên xe/<mã xe>/vitri (QoS 0, 1 điểm/giây, retain để khách vào
// sau nhận ngay điểm cuối), khách SUBSCRIBE đúng chủ đề chuyến của mình. Hỗ trợ CONNECT/CONNACK (client id,
// keepalive, Last Will), PUBLISH QoS 0 và 1 (QoS 1 trả PUBACK, chuyển tiếp ở QoS 0), SUBSCRIBE/SUBACK (ký tự đại
// diện + và #), UNSUBSCRIBE/UNSUBACK, PINGREQ/PINGRESP, DISCONNECT, tin giữ lại (payload rỗng = xóa), Last Will khi
// mất kết nối bất thường hoặc quá 1,5 × keepalive không nhận gói nào. Không làm QoS 2, phiên bền, xác thực.
// Mỗi kết nối một goroutine đọc; ghi qua khóa riêng của kết nối; bảng đăng ký, tin giữ lại dùng chung một khóa.
package main

import (
	"bufio"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

const (
	mqttConnect, mqttConnack, mqttPublish, mqttPuback        = 1, 2, 3, 4
	mqttSubscribe, mqttSuback, mqttUnsubscribe, mqttUnsuback = 8, 9, 10, 11
	mqttPingreq, mqttPingresp, mqttDisconnect                = 12, 13, 14
)

// maDoDai mã hóa "độ dài còn lại" (7 bit/byte, bit 8 = còn byte sau).
func maDoDai(n int) []byte {
	var kq []byte
	for {
		b := byte(n % 128)
		n /= 128
		if n > 0 {
			b |= 0x80
		}
		kq = append(kq, b)
		if n == 0 {
			return kq
		}
	}
}

func mqttChuoi(s string) []byte {
	b := make([]byte, 2, 2+len(s))
	binary.BigEndian.PutUint16(b, uint16(len(s)))
	return append(b, s...)
}

func mqttGoi(loai, co byte, than []byte) []byte {
	b := append([]byte{loai<<4 | co}, maDoDai(len(than))...)
	return append(b, than...)
}

func goiPublish(chuDe string, noiDung []byte, retain bool) []byte {
	var co byte
	if retain {
		co = 1
	}
	return mqttGoi(mqttPublish, co, append(mqttChuoi(chuDe), noiDung...))
}

// docGoi đọc một gói: loại, cờ, thân.
func docGoi(r *bufio.Reader) (byte, byte, []byte, error) {
	b0, err := r.ReadByte()
	if err != nil {
		return 0, 0, nil, err
	}
	n, nhan := 0, 1
	for i := 0; ; i++ {
		if i == 4 {
			return 0, 0, nil, errors.New("độ dài còn lại quá 4 byte")
		}
		b, err := r.ReadByte()
		if err != nil {
			return 0, 0, nil, err
		}
		n += int(b&0x7F) * nhan
		nhan *= 128
		if b&0x80 == 0 {
			break
		}
	}
	than := make([]byte, n)
	if _, err := io.ReadFull(r, than); err != nil {
		return 0, 0, nil, err
	}
	return b0 >> 4, b0 & 0x0F, than, nil
}

// docThan đọc lần lượt các trường trong thân gói; vượt quá độ dài -> loi.
type docThan struct {
	b   []byte
	i   int
	loi bool
}

func (d *docThan) u8() byte {
	if d.i+1 > len(d.b) {
		d.loi = true
		return 0
	}
	d.i++
	return d.b[d.i-1]
}

func (d *docThan) u16() int {
	if d.i+2 > len(d.b) {
		d.loi = true
		return 0
	}
	d.i += 2
	return int(binary.BigEndian.Uint16(d.b[d.i-2:]))
}

func (d *docThan) nhiPhan() []byte {
	n := d.u16()
	if d.loi || d.i+n > len(d.b) {
		d.loi = true
		return nil
	}
	d.i += n
	return d.b[d.i-n : d.i]
}

func (d *docThan) chuoi() string { return string(d.nhiPhan()) }
func (d *docThan) con() bool     { return !d.loi && d.i < len(d.b) }

// khop: chủ đề có khớp bộ lọc không (+ = đúng một cấp, # = mọi cấp còn lại, kể cả cấp cha).
func khop(boLoc, chuDe string) bool {
	if strings.HasPrefix(chuDe, "$") && (strings.HasPrefix(boLoc, "+") || strings.HasPrefix(boLoc, "#")) {
		return false
	}
	l, c := strings.Split(boLoc, "/"), strings.Split(chuDe, "/")
	for i, p := range l {
		if p == "#" {
			return true
		}
		if i >= len(c) || (p != "+" && p != c[i]) {
			return false
		}
	}
	return len(l) == len(c)
}

type ketNoiMqtt struct {
	conn    net.Conn
	ma      string
	dangKy  map[string]bool
	diChuc  *[3]any // chủ đề, nội dung, retain
	ghiKhoa sync.Mutex
}

func (k *ketNoiMqtt) gui(b []byte) {
	k.ghiKhoa.Lock()
	defer k.ghiKhoa.Unlock()
	k.conn.SetWriteDeadline(time.Now().Add(5 * time.Second))
	k.conn.Write(b)
}

type noiBo struct {
	boLoc string
	ham   func(chuDe string, noiDung []byte)
}

type MqttBroker struct {
	bao    func(string)
	khoa   sync.Mutex
	ketNoi map[string]*ketNoiMqtt
	giuLai map[string][]byte
	noiBo  []noiBo
	soTin  atomic.Int64
}

func moiMqttBroker(bao func(string)) *MqttBroker {
	return &MqttBroker{bao: bao, ketNoi: map[string]*ketNoiMqtt{}, giuLai: map[string][]byte{}}
}

func (b *MqttBroker) dangKyNoiBo(boLoc string, ham func(string, []byte)) {
	b.noiBo = append(b.noiBo, noiBo{boLoc, ham})
}

func (b *MqttBroker) soKetNoi() int {
	b.khoa.Lock()
	defer b.khoa.Unlock()
	return len(b.ketNoi)
}

// phat chuyển một tin tới mọi người đăng ký khớp (cờ retain = 0 khi chuyển tiếp, chuẩn 3.3.1.3).
func (b *MqttBroker) phat(chuDe string, noiDung []byte, retain bool) {
	b.soTin.Add(1)
	goi := goiPublish(chuDe, noiDung, false)
	b.khoa.Lock()
	if retain {
		if len(noiDung) > 0 {
			b.giuLai[chuDe] = append([]byte(nil), noiDung...)
		} else {
			delete(b.giuLai, chuDe)
		}
	}
	var nhan []*ketNoiMqtt
	for _, k := range b.ketNoi {
		for f := range k.dangKy {
			if khop(f, chuDe) {
				nhan = append(nhan, k)
				break
			}
		}
	}
	b.khoa.Unlock()
	for _, k := range nhan {
		k.gui(goi)
	}
	for _, nb := range b.noiBo {
		if khop(nb.boLoc, chuDe) {
			nb.ham(chuDe, noiDung)
		}
	}
}

func (b *MqttBroker) batDau(diaChi string) (net.Listener, error) {
	ln, err := net.Listen("tcp", diaChi)
	if err != nil {
		return nil, err
	}
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			go b.phucVu(c)
		}
	}()
	return ln, nil
}

func (b *MqttBroker) phucVu(c net.Conn) {
	r := bufio.NewReader(c)
	k := &ketNoiMqtt{conn: c, dangKy: map[string]bool{}}
	dongDep := false
	defer func() {
		c.Close()
		b.khoa.Lock()
		cuaToi := k.ma != "" && b.ketNoi[k.ma] == k
		if cuaToi {
			delete(b.ketNoi, k.ma)
		}
		b.khoa.Unlock()
		if cuaToi {
			if dongDep {
				b.bao(k.ma + " ngắt kết nối")
			} else {
				b.bao(k.ma + " ngắt kết nối bất thường")
				if k.diChuc != nil { // Last Will chỉ phát khi mất kết nối bất thường
					b.phat(k.diChuc[0].(string), k.diChuc[1].([]byte), k.diChuc[2].(bool))
				}
			}
		}
	}()
	c.SetReadDeadline(time.Now().Add(10 * time.Second))
	loai, _, than, err := docGoi(r)
	if err != nil || loai != mqttConnect {
		return
	}
	d := &docThan{b: than}
	tenGt, muc := d.chuoi(), d.u8()
	co, keepalive := d.u8(), d.u16()
	if tenGt != "MQTT" || muc != 4 {
		k.gui(mqttGoi(mqttConnack, 0, []byte{0, 1})) // 1 = không hỗ trợ phiên bản giao thức
		return
	}
	k.ma = d.chuoi()
	if k.ma == "" {
		k.ma = fmt.Sprintf("tu_dong_%p", k)
	}
	if co&0x04 != 0 {
		chuDe, nd := d.chuoi(), d.nhiPhan()
		k.diChuc = &[3]any{chuDe, append([]byte(nil), nd...), co&0x20 != 0}
	}
	if co&0x80 != 0 {
		d.chuoi() // tên người dùng: bỏ qua (lab không xác thực)
	}
	if co&0x40 != 0 {
		d.nhiPhan()
	}
	if d.loi {
		return
	}
	b.khoa.Lock()
	if cu := b.ketNoi[k.ma]; cu != nil { // cùng client id: ngắt kết nối cũ (đúng chuẩn)
		cu.conn.Close()
	}
	b.ketNoi[k.ma] = k
	b.khoa.Unlock()
	k.gui(mqttGoi(mqttConnack, 0, []byte{0, 0}))
	b.bao(fmt.Sprintf("%s (%s) kết nối, keepalive %d s", k.ma, c.RemoteAddr(), keepalive))
	for {
		if keepalive > 0 {
			c.SetReadDeadline(time.Now().Add(time.Duration(keepalive) * 1500 * time.Millisecond))
		} else {
			c.SetReadDeadline(time.Time{})
		}
		loai, co, than, err := docGoi(r)
		if err != nil {
			return
		}
		d := &docThan{b: than}
		switch loai {
		case mqttPublish:
			qos, retain := (co>>1)&3, co&1 != 0
			chuDe := d.chuoi()
			if qos > 0 {
				ma := d.u16()
				k.gui(mqttGoi(mqttPuback, 0, []byte{byte(ma >> 8), byte(ma)}))
			}
			if d.loi {
				return
			}
			b.phat(chuDe, append([]byte(nil), than[d.i:]...), retain)
		case mqttSubscribe:
			ma := d.u16()
			tra := []byte{byte(ma >> 8), byte(ma)}
			var moi []string
			for d.con() {
				f := d.chuoi()
				d.u8()
				moi = append(moi, f)
				tra = append(tra, 0) // cấp QoS 0
			}
			b.khoa.Lock()
			for _, f := range moi {
				k.dangKy[f] = true
			}
			var giu [][]byte
			for chuDe, nd := range b.giuLai { // tin giữ lại khớp bộ lọc mới
				for _, f := range moi {
					if khop(f, chuDe) {
						giu = append(giu, goiPublish(chuDe, nd, true))
						break
					}
				}
			}
			b.khoa.Unlock()
			k.gui(mqttGoi(mqttSuback, 0, tra))
			for _, g := range giu {
				k.gui(g)
			}
		case mqttUnsubscribe:
			ma := d.u16()
			b.khoa.Lock()
			for d.con() {
				delete(k.dangKy, d.chuoi())
			}
			b.khoa.Unlock()
			k.gui(mqttGoi(mqttUnsuback, 0, []byte{byte(ma >> 8), byte(ma)}))
		case mqttPingreq:
			k.gui(mqttGoi(mqttPingresp, 0, nil))
		case mqttDisconnect:
			dongDep = true
			return
		}
	}
}
