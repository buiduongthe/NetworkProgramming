// Bài 7.5 – Gọi dịch vụ gRPC tra cứu thuật ngữ (phía client).
//
// Cài đặt Mã giả 7.6, phần "Phía client" (Mục 7.4.3): Lookup có hạn chót (deadline), Watch nhận luồng.
// Hàm NewGlossaryClient do protoc-gen-go-grpc sinh từ tra_cuu.proto. Lỗi được báo bằng mã trạng thái gRPC.
//
// Chạy:  out/app client [host] [port] [hạn_chót_ms]   (mặc định 127.0.0.1 5001 2000)
package main

import (
	"context"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
	"time"
	"unicode"

	pb "github.com/buiduongthe/NetworkProgramming/go/Chuong07/Bai7.5_gRPC/glossaryv1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/proto"
)

// codeName: tên mã trạng thái theo đặc tả gRPC, ví dụ NotFound → NOT_FOUND.
func codeName(err error) (string, string) {
	st := status.Convert(err)
	var b strings.Builder
	for i, r := range st.Code().String() {
		if i > 0 && unicode.IsUpper(r) {
			b.WriteByte('_')
		}
		b.WriteRune(unicode.ToUpper(r))
	}
	return b.String(), st.Message()
}

func lookup(c pb.GlossaryClient, term string, deadlineMs int) {
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(deadlineMs)*time.Millisecond) // deadline ← 2000
	defer cancel()
	t, err := c.Lookup(ctx, &pb.LookupRequest{Term: term})
	if err != nil {
		code, msg := codeName(err)
		fmt.Printf("[client] Lookup('%s') -> lỗi %s: %s\n", term, code, msg)
		return
	}
	fmt.Printf("[client] Lookup('%s') -> %s: %s\n", term, t.Term, t.Text)
}

func clientMain(args []string) {
	host, port, deadlineMs := "127.0.0.1", 5001, 2000
	if len(args) >= 1 {
		host = args[0]
	}
	if len(args) >= 2 {
		port = mustPort(args[1])
	}
	if len(args) >= 3 {
		deadlineMs, _ = strconv.Atoi(args[2])
	}
	wire, _ := proto.Marshal(&pb.LookupRequest{Term: "TCP"}) // mã hóa Protocol Buffers (Mục 7.4.2)
	fmt.Printf("[client] LookupRequest{term: \"TCP\"} = % X (%d byte)\n", wire, len(wire))
	// GRPC_CHANNEL: HTTP/2 không TLS (h2c) trên máy cục bộ
	ch, err := grpc.NewClient(net.JoinHostPort(host, strconv.Itoa(port)), grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		fail("[client] " + err.Error())
	}
	defer ch.Close()
	c := pb.NewGlossaryClient(ch) // GlossaryClient(ch)
	fmt.Printf("[client] Gọi Lookup với hạn chót %d ms\n", deadlineMs)
	lookup(c, "TCP", deadlineMs)
	lookup(c, "XYZ", deadlineMs)
	fmt.Println("[client] Gọi Watch(prefix='HTTP'), nhận luồng:")
	stream, err := c.Watch(context.Background(), &pb.WatchRequest{Prefix: "HTTP"})
	for err == nil {
		var x *pb.Term
		x, err = stream.Recv() // FOR EACH x IN AWAIT_STREAM(c.Watch(...))
		if err == nil {
			fmt.Printf("[client]   %s – %s\n", x.Term, x.Text)
		}
	}
	if err != io.EOF {
		code, msg := codeName(err)
		fmt.Printf("[client] Watch -> lỗi %s: %s\n", code, msg)
	}
	fmt.Println("[client] Kết thúc")
}
