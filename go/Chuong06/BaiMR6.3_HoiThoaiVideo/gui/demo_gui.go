package main

// demo_gui – kịch bản tự thao tác trong MỘT tiến trình: cửa sổ "Camera Server" và 3 "Camera Client"
// (A, B, C dùng "Camera giả lập"), "bấm" các nút như người dùng (gọi đúng hàm xử lý trên luồng giao
// diện qua fyne.DoAndWait), in nhật ký + số liệu (fps, kbit/s), chụp các cửa sổ thành ảnh PNG.

import (
	"fmt"
	"os"
	"strings"
	"time"

	"fyne.io/fyne/v2"

	vd "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.3_HoiThoaiVideo/video"
)

func chayDemo(a fyne.App, host string, tcp, udp int, anhRa string) {
	t0 := time.Now()
	in := func(format string, v ...any) {
		fmt.Printf("[demo %4.1fs] "+format+"\n", append([]any{time.Since(t0).Seconds()}, v...)...)
	}
	s := moCuaSoServer(a, host, tcp, udp)
	ten := []string{"A", "B", "C"}
	var ds []*cuaSoClient
	for _, t := range ten {
		ds = append(ds, moCuaSoClient(a, t, host, tcp))
	}
	cho := func(moTa string, dk func() bool) {
		for i := 0; i < 100; i++ {
			ok := false
			fyne.DoAndWait(func() { ok = dk() })
			if ok {
				return
			}
			time.Sleep(50 * time.Millisecond)
		}
		in("LỖI: quá thời gian chờ %s", moTa)
	}
	tuongTac := func(f func()) { fyne.DoAndWait(f); time.Sleep(150 * time.Millisecond) }
	chonClient := func(tenK string) {
		for i, t := range s.ds {
			if t.Ten == tenK {
				s.danhSach.Select(i)
			}
		}
	}

	go func() {
		time.Sleep(time.Second)
		in("ffmpeg: %v, camera thật tìm thấy: %d", vd.CoLenh("ffmpeg"), len(vd.LietKeCamera()))
		in("Server: bấm START SERVER (%s:%d, UDP %d)", host, tcp, udp)
		tuongTac(func() { bam(s.batDau) })
		cho("server chạy", func() bool { return s.phong != nil })
		for i, c := range ds {
			in("Client %s: bấm Kết nối", ten[i])
			tuongTac(func() { bam(c.ketNoi) })
			cho("client "+ten[i]+" kết nối", func() bool { return c.khach != nil })
		}
		for i, c := range ds {
			in("Client %s: chọn \"%s\", bấm START", ten[i], c.chonCam.Selected)
			tuongTac(func() { bam(c.batCam) })
		}
		time.Sleep(2500 * time.Millisecond)
		inBang(s, ds, in)

		in("Server: chọn B trong danh sách, bấm \"Tắt camera\"")
		tuongTac(func() { chonClient("B"); bam(s.nutCamera) })
		in("Server: chọn C, đặt fps gửi tối đa 5, độ phân giải gửi 1/2")
		tuongTac(func() { chonClient("C"); s.fpsGui.SetSelected("5"); s.phanGiai.SetSelected("1/2") })
		in("Client B: bấm CANCEL rồi START -> server từ chối (đang tắt camera của B)")
		tuongTac(func() { bam(ds[1].tatCam) })
		time.Sleep(300 * time.Millisecond)
		tuongTac(func() { bam(ds[1].batCam) })
		in("Client C: chọn nhận 5 fps, độ phân giải nhận 1/2")
		tuongTac(func() { ds[2].fpsNhan.SetSelected("5"); ds[2].phanGiaiNhan.SetSelected("1/2") })
		tuongTac(func() { chonClient("C") })
		time.Sleep(3 * time.Second)
		inBang(s, ds, in)

		in("Chụp các cửa sổ…")
		ws := []fyne.Window{s.w}
		for _, c := range ds {
			ws = append(ws, c.w)
		}
		anh := chupCuaSo(ws)
		var tieuDe []string
		fyne.DoAndWait(func() {
			for _, w := range ws {
				tieuDe = append(tieuDe, w.Title())
			}
		})
		W, H, err := ghepAnhChup(a, anh[:2], tieuDe[:2], anh[2:], tieuDe[2:], anhRa)
		if err != nil {
			in("Lỗi ghi ảnh: %v", err)
		} else {
			st, _ := os.Stat(anhRa)
			in("Đã ghi %s (%dx%d, %d KB)", anhRa, W, H, st.Size()/1024)
		}
		in("Dừng: client Ngắt kết nối, server STOP SERVER")
		fyne.DoAndWait(func() {
			for _, c := range ds {
				bam(c.ngat)
			}
			bam(s.dung)
		})
		time.Sleep(300 * time.Millisecond)
		fyne.Do(a.Quit)
	}()
	s.w.Show()
	for _, c := range ds {
		c.w.Show()
	}
	a.Run()
}

// inBang – in trạng thái server (bảng client) và thanh trạng thái từng client, đọc từ giao diện.
func inBang(s *cuaSoServer, ds []*cuaSoClient, in func(string, ...any)) {
	fyne.DoAndWait(func() {
		in("Server: %s", strings.TrimPrefix(s.trangThai.Text, "Trạng thái: "))
		for i, t := range s.ds {
			in("  ô %d: #%d %s camera %-13s gửi ≤ %2d fps 1/%d | vào %2.0f fps %4.0f kbit/s | lưới ra %2.0f fps %4.0f kbit/s (nhận ≤ %d fps, 1/%d)",
				i+1, t.Ma, t.Ten, t.LyDo, t.FpsToiDa, t.Chia, t.FpsVao, t.KbpsVao, t.FpsRa, t.KbpsRa, t.NhanFps, t.NhanChia)
		}
		for _, c := range ds {
			t := c.TrangThaiCuoi
			in("  client %s: %s | lưới %dx%d, khung đủ %d, mảnh mất %d", c.ten.Text,
				strings.TrimPrefix(c.trangThai.Text, "Trạng thái: "), t.LuoiW, t.LuoiH, t.KhungDu, t.ManhMat)
		}
	})
}
