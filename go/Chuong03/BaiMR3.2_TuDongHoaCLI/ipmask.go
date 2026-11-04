package main

import (
	"encoding/binary"
	"fmt"
	"math/bits"
	"net"
	"net/netip"
	"os"
	"os/signal"
)

// maskBits – đổi mặt nạ chấm thập phân (255.255.255.0) thành độ dài tiền tố (24);
// trả về -1 nếu mặt nạ không hợp lệ (các bit 1 không liên tục).
func maskBits(mask netip.Addr) int {
	if !mask.Is4() {
		return -1
	}
	b := mask.As4()
	v := binary.BigEndian.Uint32(b[:])
	n := bits.LeadingZeros32(^v) // số bit 1 liên tiếp ở đầu
	if v<<n != 0 {
		return -1
	}
	return n
}

// bitsToMask – đổi độ dài tiền tố thành mặt nạ chấm thập phân.
func bitsToMask(n int) netip.Addr {
	var b [4]byte
	binary.BigEndian.PutUint32(b[:], ^uint32(0)<<(32-n))
	return netip.AddrFrom4(b)
}

// stopOnInterrupt – khi nhận Ctrl+C (SIGINT): in thông báo và đóng listener để vòng Accept kết thúc.
func stopOnInterrupt(ln net.Listener, msg string) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		fmt.Println(msg)
		ln.Close()
	}()
}
