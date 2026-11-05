package main

// demo_gui – kịch bản tự thao tác trong MỘT tiến trình: mở cửa sổ server và 3 cửa sổ client (A, B,
// C – nguồn giả lập sin 440, 620, 880 Hz), "bấm" các nút như người dùng (gọi đúng hàm xử lý của nút
// trên luồng giao diện qua fyne.DoAndWait), in nhật ký các bước + số liệu đo, chụp các cửa sổ thành
// ảnh PNG. Cuối cùng phân tích 2 giây tiếng A nghe được bằng Goertzel để kiểm tra bộ trộn.

import (
	"fmt"
	"math"
	"os"
	"time"

	"fyne.io/fyne/v2"

	ht "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.2_HoiThoaiAmThanh/amthanh"
)

func chayDemo(a fyne.App, host string, tcp, udp int, anhRa string) {
	t0 := time.Now()
	in := func(format string, v ...any) {
		fmt.Printf("[demo %4.1fs] "+format+"\n", append([]any{time.Since(t0).Seconds()}, v...)...)
	}
	s := moCuaSoServer(a, host, tcp, udp)
	type kh struct {
		ten string
		f   float64
		c   *cuaSoClient
	}
	ds := []*kh{{"A", 440, nil}, {"B", 620, nil}, {"C", 880, nil}}
	for _, k := range ds {
		k.c = moCuaSoClient(a, k.ten, host, tcp, k.f)
		k.c.w.Resize(fyne.NewSize(440, 720))
	}
	s.w.Resize(fyne.NewSize(1340, 690))
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

	go func() {
		time.Sleep(time.Second) // chờ các cửa sổ hiện
		in("ffmpeg: %v, ffplay: %v, micro thật tìm thấy: %d", ht.CoLenh("ffmpeg"), ht.CoLenh("ffplay"), len(ht.LietKeMicro()))
		in("Server: bấm Start Server (%s:%d, UDP %d)", host, tcp, udp)
		tuongTac(func() { bam(s.batDau) })
		cho("server chạy", func() bool { return s.phong != nil })
		for _, k := range ds {
			in("Client %s: bấm Connect", k.ten)
			tuongTac(func() { bam(k.c.ketNoi) })
			cho("client "+k.ten+" kết nối", func() bool { return k.c.khach != nil })
		}
		for _, k := range ds {
			in("Client %s: chọn \"%s\", bấm Start Mic", k.ten, k.c.chonMic.Selected)
			tuongTac(func() { bam(k.c.batMic) })
		}
		in("Client A: bấm Start Audio (phát ra loa bằng ffplay nếu có thiết bị)")
		tuongTac(func() { bam(ds[0].c.batAudio) })
		in("Server: Mic Volume 50%%, chọn \"%s\", bấm Start Mic", tenGiaLapServer)
		tuongTac(func() { s.heSoMic.SetValue(50); bam(s.batMic) })
		in("Server: bật Nghe (phát tiếng client ra loa server)")
		tuongTac(func() { s.nghe.SetChecked(true) })
		time.Sleep(2 * time.Second)
		inBang(s, ds[0].c, in)

		in("Server: bấm \"Khóa mic\" ở hàng B; kéo độ lợi C lên 150%%")
		tuongTac(func() {
			for _, h := range s.hang {
				if ten(h) == "B" {
					bam(h.khoa)
				}
				if ten(h) == "C" {
					h.doLoi.SetValue(150)
				}
			}
		})
		in("Client B: bấm Stop Mic rồi Start Mic -> server từ chối (đang khóa)")
		tuongTac(func() { bam(ds[1].c.tatMic) })
		time.Sleep(300 * time.Millisecond)
		tuongTac(func() { bam(ds[1].c.batMic) })
		in("Client C: kéo âm lượng nghe xuống 70%% (chỉ áp dụng tại C, server chỉ hiển thị)")
		tuongTac(func() { ds[2].c.amLuong.SetValue(70) })
		time.Sleep(2500 * time.Millisecond)
		inBang(s, ds[0].c, in)
		fyne.DoAndWait(func() {
			in("Client B: \"%s\" | %s", ds[1].c.micTT.Text, ds[1].c.trangThai.Text)
			in("Client C: \"%s\" (server báo bằng thông điệp do_loi)", ds[2].c.serverVol.Text)
		})

		// Kiểm tra bộ trộn: A phải nghe server (330 Hz, 50%) và C (880 Hz, ×1,5); không nghe chính mình
		// (mix-minus, 440 Hz) và không nghe B (bị khóa, 620 Hz). Sin 0,25 toàn thang = -15,1 dBFS.
		var mau []int16
		fyne.DoAndWait(func() { mau = ds[0].c.khach.MauGanDay() })
		in("Phân tích 2 s tiếng A nghe (Goertzel):")
		for _, am := range []struct {
			f       float64
			nhan    string
			mongDoi string
		}{{330, "server (Mic Volume 50%)", "-21.1"}, {440, "A – chính mình", "không có"},
			{620, "B – bị khóa", "không có"}, {880, "C – độ lợi 150%", "-11.6"}} {
			db := ht.MucAmDBFS(mau, am.f)
			chu := "---"
			if db > -60 {
				chu = fmt.Sprintf("%.1f", db)
			}
			in("  %4.0f Hz %-26s %7s dBFS (mong đợi %s)", am.f, am.nhan, chu, am.mongDoi)
		}

		in("Chụp các cửa sổ…")
		ws := []fyne.Window{s.w}
		for _, k := range ds {
			ws = append(ws, k.c.w)
		}
		anh := chupCuaSo(ws)
		var tieuDe []string
		fyne.DoAndWait(func() {
			for _, w := range ws {
				tieuDe = append(tieuDe, w.Title())
			}
		})
		W, H, err := ghepAnhChup(a, anh[:1], tieuDe[:1], anh[1:], tieuDe[1:], anhRa)
		if err != nil {
			in("Lỗi ghi ảnh: %v", err)
		} else {
			st, _ := os.Stat(anhRa)
			in("Đã ghi %s (%dx%d, %d KB)", anhRa, W, H, st.Size()/1024)
		}
		in("Dừng: client Disconnect, server Stop Server")
		fyne.DoAndWait(func() {
			for _, k := range ds {
				bam(k.c.ngat)
			}
			bam(s.dung)
		})
		time.Sleep(300 * time.Millisecond)
		fyne.Do(a.Quit)
	}()
	s.w.Show()
	for _, k := range ds {
		k.c.w.Show()
	}
	a.Run()
}

func ten(h *hangKhach) string {
	var so int
	var t string
	fmt.Sscanf(h.ten.Text, "#%d %s", &so, &t)
	return t
}

// inBang – in bảng trạng thái server (đọc từ chính các widget, như người nhìn màn hình) và mức A nhận.
func inBang(s *cuaSoServer, a *cuaSoClient, in func(string, ...any)) {
	fyne.DoAndWait(func() {
		in("Server: %s | %s | micro server %s | nghe (tổng client) %s", s.trangThai.Text, s.soKhach.Text,
			chuDb(s.mucMic.db), chuDb(s.mucNghe.db))
		in("  Nghe: %s", s.nhanNghe.Text)
		for _, h := range sapXep(s.hang) {
			in("  %-34s %-24s vào %s  %s  %s", h.ten.Text, h.mic.Text, chuDb(h.muc.db), h.nhanDoLoi.Text, h.nghe.Text)
		}
		in("Client A: tiếng nhận %s, mic %s, %s, %s", chuDb(a.mucNhan.db), chuDb(a.mucMic.db), a.serverVol.Text, a.nhanLoa.Text)
	})
}

func sapXep(m map[uint32]*hangKhach) []*hangKhach {
	var ds []*hangKhach
	for i := uint32(1); len(ds) < len(m) && i < 1000; i++ {
		if h, ok := m[i]; ok {
			ds = append(ds, h)
		}
	}
	return ds
}

func chuDb(db float64) string {
	if math.IsInf(db, -1) || db < -90 {
		return "  --- dBFS"
	}
	return fmt.Sprintf("%5.1f dBFS", db)
}
