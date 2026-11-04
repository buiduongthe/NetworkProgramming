package main

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// phanTich – đo mức từng âm (Goertzel) trong từng đoạn của bản ghi. Đoạn được chia theo các mốc
// sự kiện client đã lưu (<tên>_moc.json); bỏ 0,15 s đầu mỗi đoạn vì âm thanh tới tai người nghe
// trễ hơn thông báo TCP (độ sâu bộ đệm jitter + một nhịp trộn). Không có tệp mốc: chia mỗi 1 s.
func phanTich(wav string, dsAm []string) {
	const boDau, nganNhat, nguong = 0.15, 0.3, -50.0
	type am struct {
		f    float64
		nhan string
	}
	var ams []am
	for _, x := range dsAm {
		fs, n, _ := strings.Cut(x, ":")
		f, _ := strconv.ParseFloat(fs, 64)
		ams = append(ams, am{f, n})
	}
	mau, err := docWAV(wav)
	if err != nil {
		fmt.Println("[phan_tich] Lỗi:", err)
		os.Exit(1)
	}
	dai := float64(len(mau)) / tanSoMau
	fmt.Printf("[phan_tich] %s: %.2f s, 16000 Hz, 16 bit, mono\n", wav, dai)
	fmt.Printf("[phan_tich] Goertzel, cửa sổ Hann; RMS dBFS; --- = vắng (<%g)\n", nguong)

	// ---- Mốc chia đoạn
	type mocT struct {
		t  float64
		ds []string
	}
	var dsMoc []moc
	tenMinh := ""
	if b, err := os.ReadFile(strings.TrimSuffix(wav, filepath.Ext(wav)) + "_moc.json"); err == nil {
		var d struct {
			Ten string `json:"ten"`
			Moc []moc  `json:"moc"`
		}
		json.Unmarshal(b, &d)
		tenMinh = d.Ten
		dsMoc = append([]moc{{0, "bắt đầu ghi"}}, d.Moc...)
	} else {
		for i := 0; i < int(dai); i++ {
			dsMoc = append(dsMoc, moc{float64(i), fmt.Sprintf("cửa sổ %d", i+1)})
		}
	}
	var nhom []mocT // gộp các mốc quá gần nhau thành một
	for _, m := range dsMoc {
		if len(nhom) > 0 && m.T-nhom[len(nhom)-1].t < boDau+nganNhat {
			nhom[len(nhom)-1].ds = append(nhom[len(nhom)-1].ds, m.SuKien)
		} else {
			nhom = append(nhom, mocT{m.T, []string{m.SuKien}})
		}
	}

	// ---- Đo mức từng âm trong từng đoạn
	tieuDe := "Đoạn  Từ-đến (s) "
	for _, a := range ams {
		tieuDe += fmt.Sprintf("%2s %-4g", a.nhan, a.f)
	}
	fmt.Println(tieuDe + " Thay đổi")
	var truoc []float64
	var ketQua [][]float64
	vang := math.Inf(-1)
	for i, g := range nhom {
		dau := g.t + boDau
		cuoi := dai
		if i+1 < len(nhom) {
			cuoi = nhom[i+1].t
		}
		if cuoi-dau < 0.1 {
			continue
		}
		doan := mau[int(dau*tanSoMau):int(cuoi*tanSoMau)]
		muc := make([]float64, len(ams))
		dong := fmt.Sprintf("%3d   %4.2f-%4.2f ", i+1, dau, cuoi)
		for j, a := range ams {
			muc[j] = mucAmDBFS(doan, a.f)
			if muc[j] < nguong {
				muc[j] = vang
			}
			dong += chuDB(muc[j], 7)
		}
		var doi []string
		if truoc != nil {
			for j, a := range ams {
				m0, m1 := truoc[j], muc[j]
				switch {
				case m0 == vang && m1 > vang:
					doi = append(doi, a.nhan+" bật")
				case m0 > vang && m1 == vang:
					doi = append(doi, a.nhan+" tắt")
				case m0 > vang && math.Abs(m1-m0) >= 1:
					doi = append(doi, fmt.Sprintf("%s %+.1fdB", a.nhan, m1-m0))
				}
			}
		}
		fmt.Println(dong + "  " + strings.Join(doi, ", "))
		truoc = muc
		ketQua = append(ketQua, muc)
	}
	fmt.Println("Sự kiện mở đầu mỗi đoạn:")
	for i, g := range nhom {
		fmt.Printf("%3d %5.2fs %s\n", i+1, g.t, strings.Join(g.ds, "; "))
	}

	// ---- Kiểm tra mix-minus: người nghe không được nghe tiếng của chính mình
	for j, a := range ams {
		if a.nhan != tenMinh {
			continue
		}
		lonNhat := vang
		for _, m := range ketQua {
			lonNhat = math.Max(lonNhat, m[j])
		}
		kq := "vắng mọi đoạn -> ĐÚNG"
		if lonNhat > vang {
			kq = fmt.Sprintf("%.1f dBFS -> SAI", lonNhat)
		}
		fmt.Printf("Mix-minus: %s không nghe tiếng mình (%g Hz): %s\n", a.nhan, a.f, kq)
	}
}
