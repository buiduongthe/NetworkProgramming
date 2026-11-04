// Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (phía server).
//
// Cài đặt Mã giả 7.6, phần CLASS GlossaryService (Mục 7.4.3, Giáo trình Lập trình mạng):
// Lookup (unary, NOT_FOUND khi không có) và Watch (luồng từ server, dừng khi client hủy).
// Kế thừa: Mã giả 3.7 – nhánh "lookup" của PROCESS_REQUEST thành phương thức Lookup.
// Kiểu UnimplementedGlossaryServer và hàm RegisterGlossaryServer do protoc-gen-go-grpc sinh từ tra_cuu.proto
// (thư mục glossaryv1/, xem README).
//
// Chạy:  out/app server [host] [port] [độ_trễ_ms]   (mặc định 0.0.0.0 5001 0)
package main

import (
	"context"
	"fmt"
	"net"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"time"

	pb "github.com/buiduongthe/NetworkProgramming/go/Chuong07/Bai7.5_gRPC/glossaryv1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

var DICT = map[string]string{"TCP": "Giao thức vận chuyển hướng kết nối", "UDP": "Giao thức vận chuyển không kết nối"}
var CHANGES = []string{"HTTP/2", "QUIC", "HTTP/3", "gRPC", "HTTPS"} // dòng thay đổi mô phỏng

// changesFeed – CHANGES_FEED(): mỗi 0,5 giây có một thuật ngữ mới; dừng sớm khi ctx bị hủy.
func changesFeed(ctx context.Context) <-chan *pb.Term {
	out := make(chan *pb.Term)
	go func() {
		defer close(out)
		for _, term := range CHANGES {
			select {
			case <-time.After(500 * time.Millisecond):
			case <-ctx.Done():
				return
			}
			out <- &pb.Term{Term: term, Text: "Thuật ngữ mới: " + term, Tags: []string{"moi"}}
		}
	}()
	return out
}

// GlossaryService – CLASS GlossaryService EXTENDS GlossaryBase: nhúng kiểu do công cụ sinh.
type GlossaryService struct {
	pb.UnimplementedGlossaryServer
	delayMs int // dùng để thử hạn chót (deadline)
}

// Lookup – ASYNC FUNCTION Lookup(req, ctx).
func (s *GlossaryService) Lookup(ctx context.Context, req *pb.LookupRequest) (*pb.Term, error) {
	if s.delayMs > 0 {
		select {
		case <-time.After(time.Duration(s.delayMs) * time.Millisecond): // server cố ý chậm
		case <-ctx.Done(): // hạn chót truyền qua grpc-timeout
			fmt.Printf("[server] Lookup('%s') bị hủy: client đã hết hạn chót\n", req.Term)
			return nil, ctx.Err()
		}
	}
	text, ok := DICT[req.Term]
	if !ok {
		fmt.Printf("[server] Lookup('%s') -> NOT_FOUND\n", req.Term)
		return nil, status.Error(codes.NotFound, "Không có "+req.Term) // RAISE RpcError(NOT_FOUND, …)
	}
	fmt.Printf("[server] Lookup('%s') -> OK\n", req.Term)
	return &pb.Term{Term: req.Term, Text: text}, nil
}

// Watch – ASYNC PROCEDURE Watch(req, stream, ctx): luồng từ server.
func (s *GlossaryService) Watch(req *pb.WatchRequest, stream pb.Glossary_WatchServer) error {
	ctx := stream.Context()
	fmt.Printf("[server] Watch(prefix='%s') bắt đầu\n", req.Prefix)
	for t := range changesFeed(ctx) { // FOR EACH t IN AWAIT_STREAM(CHANGES_FEED())
		if ctx.Err() != nil { // client đã hủy
			return nil
		}
		if strings.HasPrefix(t.Term, req.Prefix) {
			if err := stream.Send(t); err != nil { // AWAIT stream.write(t)
				return err
			}
			fmt.Printf("[server] Watch: gửi %s\n", t.Term)
		}
	}
	fmt.Println("[server] Watch kết thúc")
	return nil
}

func serverMain(args []string) {
	host, port, delayMs := "0.0.0.0", 5001, 0
	if len(args) >= 1 {
		host = args[0]
	}
	if len(args) >= 2 {
		port = mustPort(args[1])
	}
	if len(args) >= 3 {
		delayMs, _ = strconv.Atoi(args[2])
	}
	lis, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	srv := grpc.NewServer() // HTTP/2 không TLS (h2c) cho phòng thí nghiệm
	pb.RegisterGlossaryServer(srv, &GlossaryService{delayMs: delayMs})
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() { // Ctrl+C: dừng gọn
		<-ch
		srv.Stop()
	}()
	fmt.Printf("[server] Dịch vụ gRPC glossary.v1.Glossary tại %s:%d (độ trễ Lookup %d ms)\n", host, port, delayMs)
	srv.Serve(lis)
	fmt.Println("[server] Dừng server")
}
