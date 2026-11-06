// Gói tranhchap – khung dùng chung cho nhóm bài mở rộng "tranh chấp tài nguyên" MR6.6–MR6.14 (bài
// đầu: MR6.6 Đấu giá) và nghiệp vụ của bài.
//
// Các tệp giaothuc.go, thongke.go, kho.go, sokiemtoan.go, maychu.go, khach.go, mophong.go GIỐNG NHAU ở
// cả 9 bài (chép nguyên, như chung.py/khach.py/mo_phong.py của bản Python); nghiệp vụ riêng của từng
// bài nằm ở nghiepvu.go (máy chủ + sổ kiểm toán) và nguoiao.go (người dùng ảo của bộ mô phỏng).
//
//  1. Giao thức (tệp này): GuiJSON, MaHoa, DocDong (mỗi dòng một JSON UTF-8 kết thúc "\n", tối đa
//     64 KB), TenHopLe, DocLenhStdin ("@t lệnh"), DocThamSo ("khoa=gia_tri"); định dạng in Tien,
//     DongThongKe, BangSoSanh – các bản chuyển ngữ in GIỐNG HỆT bản Python.
//  2. Đo đạc (thongke.go): PhanVi (theo hạng gần nhất), ThongKe.
//  3. KhoTranhChap (kho.go): kho các "ô" tài nguyên và BỐN cách xử lý tranh chấp khi n goroutine cùng ghi:
//     khong_khoa  đọc – xử lý – ghi không đồng bộ        -> tranh chấp (race condition), mất cập nhật
//     khoa        khóa loại trừ theo từng ô, giữ suốt đọc – xử lý – ghi
//     lac_quan    đọc (giá trị, phiên bản) -> xử lý ngoài khóa -> so sánh-và-đổi (CAS) dưới khóa ngắn
//     hang_doi    mọi yêu cầu ghi xếp vào một channel cho MỘT goroutine xử lý (mô hình actor)
//  4. SoKiemToan (sokiemtoan.go): sổ ghi mọi giao dịch được chấp nhận, đếm sai phạm.
//  5. MayChuCoSo (maychu.go): máy chủ TCP – một goroutine mỗi kết nối; KetNoiKhach, QuanTri (khach.go);
//     bộ mô phỏng tải có rào xuất phát và pipelining (mophong.go).
//
// Giao thức (TCP, mỗi dòng một JSON) – giống hệt bản Python, xem README:
//
//	client -> server  {"loai":"chao","ten":"An","vai_tro":"nguoi_dung"|"quan_tri"[,"so_nguoi":m]}
//	                  {"loai":"<thao_tac>","ma_yc":k,["nguoi":"u17"],...}
//	                  {"loai":"che_do"|"tre"|"dat_lai"|"thong_ke","ma_yc":k,...}   (chỉ quan_tri)
//	server -> client  {"loai":"chao",...} rồi ngay một cap_nhat; {"loai":"kq","ma_yc":k,"ok":...};
//	                  {"loai":"cap_nhat",...} (gộp ≤ 10 lần/s); {"loai":"su_kien",...}; {"loai":"thong_ke",...}
package tranhchap

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"os"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

const (
	GioiHanDong   = 64 * 1024 // một dòng JSON tối đa 64 KB
	SoLanThu      = 5         // lac_quan: số lần thử tối đa
	ChoKhoaToiDaS = 2.0       // khoa: chờ một khóa quá 2 s thì bỏ (nghi khóa chết – deadlock)
	NhoKQ         = 1024      // mỗi kết nối nhớ kết quả của 1024 ma_yc gần nhất
	GiuDoTre      = 200_000   // giữ tối đa 200 000 mẫu độ trễ cho phân vị
	NhipDay       = 100 * time.Millisecond
)

// CheDoDS – bốn cách xử lý tranh chấp (thứ tự như bản Python).
var CheDoDS = []string{"khong_khoa", "khoa", "lac_quan", "hang_doi"}

var TenCheDo = map[string]string{
	"khong_khoa": "Không khóa (lỗi tranh chấp)",
	"khoa":       "Khóa loại trừ (mutex)",
	"lac_quan":   "Lạc quan (phiên bản + CAS)",
	"hang_doi":   "Hàng đợi một luồng (actor)",
}

var VaiTro = []string{"nguoi_dung", "quan_tri"}

// M – một thông điệp JSON đã giải mã (số nguyên -> int64, số thực -> float64, như int/float của Python).
type M = map[string]any

// KV, TS – đối tượng JSON GIỮ THỨ TỰ khóa (tham số đặt lại, yêu cầu gửi đi): bản Python in
// "Đặt lại (thoi_gian_s=5.0, gia_han_s=3.0)" theo đúng thứ tự trong thông điệp.
type KV struct {
	K string
	V any
}
type TS []KV

// T – TS từ các cặp khóa, giá trị xen kẽ: T("gia", 5300000, "nguoi", "An").
func T(cap ...any) TS {
	t := make(TS, 0, len(cap)/2)
	for i := 0; i+1 < len(cap); i += 2 {
		t = append(t, KV{cap[i].(string), cap[i+1]})
	}
	return t
}

func (t TS) Lay(k string) (any, bool) {
	for _, kv := range t {
		if kv.K == k {
			return kv.V, true
		}
	}
	return nil, false
}

// Dat – thay giá trị khóa đã có, không thì thêm vào cuối (như dict.update của Python).
func (t TS) Dat(k string, v any) TS {
	for i, kv := range t {
		if kv.K == k {
			t[i].V = v
			return t
		}
	}
	return append(t, KV{k, v})
}

func (t TS) MarshalJSON() ([]byte, error) {
	var b bytes.Buffer
	maHoaVao(&b, t)
	return b.Bytes(), nil
}

// ================================================================ 1. Giao thức

// MaHoa – bytes của một dòng JSON UTF-8 kết thúc "\n" (dùng khi gửi nhiều thông điệp trong MỘT lần
// Write – pipelining).
func MaHoa(v any) []byte {
	var b bytes.Buffer
	maHoaVao(&b, v)
	b.WriteByte('\n')
	return b.Bytes()
}

// GuiJSON – mỗi thông điệp là một dòng JSON.
func GuiJSON(w io.Writer, v any) error {
	_, err := w.Write(MaHoa(v))
	return err
}

// maHoaVao – như json.dumps(ensure_ascii=False) của Python: không thoát <, >, &; số thực in như repr
// của Python (5.0, 0.25); M theo thứ tự khóa sắp xếp, TS theo thứ tự đưa vào.
func maHoaVao(b *bytes.Buffer, v any) {
	switch x := v.(type) {
	case nil:
		b.WriteString("null")
	case bool:
		b.WriteString(strconv.FormatBool(x))
	case string:
		maHoaChuoi(b, x)
	case int:
		b.WriteString(strconv.Itoa(x))
	case int64:
		b.WriteString(strconv.FormatInt(x, 10))
	case float64:
		b.WriteString(ReprFloat(x))
	case TS:
		b.WriteByte('{')
		for i, kv := range x {
			if i > 0 {
				b.WriteString(", ")
			}
			maHoaChuoi(b, kv.K)
			b.WriteString(": ")
			maHoaVao(b, kv.V)
		}
		b.WriteByte('}')
	case M:
		khoa := make([]string, 0, len(x))
		for k := range x {
			khoa = append(khoa, k)
		}
		sort.Strings(khoa)
		b.WriteByte('{')
		for i, k := range khoa {
			if i > 0 {
				b.WriteString(", ")
			}
			maHoaChuoi(b, k)
			b.WriteString(": ")
			maHoaVao(b, x[k])
		}
		b.WriteByte('}')
	case []any:
		b.WriteByte('[')
		for i, p := range x {
			if i > 0 {
				b.WriteString(", ")
			}
			maHoaVao(b, p)
		}
		b.WriteByte(']')
	case []string:
		b.WriteByte('[')
		for i, p := range x {
			if i > 0 {
				b.WriteString(", ")
			}
			maHoaChuoi(b, p)
		}
		b.WriteByte(']')
	case []M:
		b.WriteByte('[')
		for i, p := range x {
			if i > 0 {
				b.WriteString(", ")
			}
			maHoaVao(b, p)
		}
		b.WriteByte(']')
	default:
		enc := json.NewEncoder(b)
		enc.SetEscapeHTML(false)
		if err := enc.Encode(x); err != nil {
			b.WriteString("null")
			return
		}
		b.Truncate(b.Len() - 1) // bỏ "\n" của Encode
	}
}

func maHoaChuoi(b *bytes.Buffer, s string) {
	enc := json.NewEncoder(b)
	enc.SetEscapeHTML(false)
	_ = enc.Encode(s)
	b.Truncate(b.Len() - 1)
}

// ReprFloat – số thực in như repr() của Python: 5.0, 0.25, 1e-05, 1e+16.
func ReprFloat(x float64) string {
	if math.IsInf(x, 0) || math.IsNaN(x) {
		return map[bool]string{true: "inf", false: "-inf"}[x > 0]
	}
	a := math.Abs(x)
	if a != 0 && (a < 1e-4 || a >= 1e16) {
		s := strconv.FormatFloat(x, 'e', -1, 64)
		return s
	}
	s := strconv.FormatFloat(x, 'f', -1, 64)
	if !strings.ContainsAny(s, ".") {
		s += ".0"
	}
	return s
}

// LoiDong – dòng quá dài, bị cắt hoặc không phải đối tượng JSON (ValueError của bản Python).
var LoiDong = errors.New("dòng quá dài hoặc bị cắt")

// DocDong – đọc một dòng JSON. Trả về (nil, nil, nil) khi kết nối đóng; dòng > 64 KB hoặc sai dạng
// -> lỗi. tho là bytes của dòng (chỉ hợp lệ tới lần đọc kế tiếp) – dùng để lấy thứ tự khóa (ThuTuKhoa).
func DocDong(r *bufio.Reader) (tin M, tho []byte, err error) {
	dong, err := r.ReadSlice('\n')
	if err != nil {
		if err == io.EOF && len(dong) == 0 {
			return nil, nil, nil
		}
		if err == io.EOF || err == bufio.ErrBufferFull {
			return nil, nil, LoiDong
		}
		return nil, nil, err
	}
	tin, err = GiaiMa(dong)
	return tin, dong, err
}

// MoiBoDoc – bufio.Reader đủ lớn cho một dòng 64 KB.
func MoiBoDoc(r io.Reader) *bufio.Reader { return bufio.NewReaderSize(r, GioiHanDong) }

// GiaiMa – json.loads: số nguyên -> int64, số thực -> float64; phải là đối tượng.
func GiaiMa(dong []byte) (M, error) {
	dec := json.NewDecoder(bytes.NewReader(dong))
	dec.UseNumber()
	var v any
	if err := dec.Decode(&v); err != nil {
		return nil, fmt.Errorf("JSON sai: %v", err)
	}
	if dec.More() {
		return nil, errors.New("JSON sai: dữ liệu thừa")
	}
	m, ok := v.(map[string]any)
	if !ok {
		return nil, errors.New("thông điệp không phải đối tượng JSON")
	}
	return chuanHoa(m).(M), nil
}

func chuanHoa(v any) any {
	switch x := v.(type) {
	case json.Number:
		s := string(x)
		if !strings.ContainsAny(s, ".eE") {
			if n, err := x.Int64(); err == nil {
				return n
			}
		}
		f, _ := x.Float64()
		return f
	case map[string]any:
		for k, p := range x {
			x[k] = chuanHoa(p)
		}
		return x
	case []any:
		for i, p := range x {
			x[i] = chuanHoa(p)
		}
		return x
	}
	return v
}

// ThuTuKhoa – các khóa cấp ngoài cùng của một dòng JSON theo đúng thứ tự xuất hiện.
func ThuTuKhoa(dong []byte) []string {
	dec := json.NewDecoder(bytes.NewReader(dong))
	if t, err := dec.Token(); err != nil || t != json.Delim('{') {
		return nil
	}
	var ds []string
	for dec.More() {
		t, err := dec.Token()
		if err != nil {
			break
		}
		k, _ := t.(string)
		var bo json.RawMessage
		if dec.Decode(&bo) != nil {
			break
		}
		ds = append(ds, k)
	}
	return ds
}

// TenHopLe – chuỗi 1–16 ký tự in được, không có khoảng trắng đầu/cuối.
func TenHopLe(v any) bool {
	s, ok := v.(string)
	if !ok {
		return false
	}
	n := utf8.RuneCountInString(s)
	if n < 1 || n > 16 || strings.TrimSpace(s) != s {
		return false
	}
	for _, r := range s {
		if !unicode.IsPrint(r) {
			return false
		}
	}
	return true
}

// LaSoNguyen – số nguyên JSON (không phải bool, không phải số thực).
func LaSoNguyen(v any) bool { _, ok := v.(int64); return ok }

// SoNguyen, SoThuc, Chuoi – đọc trường số/chuỗi của thông điệp (máy chủ Python gửi 30.0, máy chủ Go
// gửi 30 – cả hai đọc được).
func SoNguyen(v any) int64 {
	switch x := v.(type) {
	case int64:
		return x
	case int:
		return int64(x)
	case float64:
		return int64(x)
	case bool:
		if x {
			return 1
		}
	}
	return 0
}

func SoThuc(v any) float64 {
	switch x := v.(type) {
	case int64:
		return float64(x)
	case int:
		return float64(x)
	case float64:
		return x
	}
	return 0
}

func Chuoi(v any) string { s, _ := v.(string); return s }

// StrPy – str() của Python cho một giá trị JSON (None, True, 5.0…): dùng khi in thông báo lỗi.
func StrPy(v any) string {
	switch x := v.(type) {
	case nil:
		return "None"
	case bool:
		if x {
			return "True"
		}
		return "False"
	case string:
		return x
	case float64:
		return ReprFloat(x)
	case int64:
		return strconv.FormatInt(x, 10)
	case int:
		return strconv.Itoa(x)
	}
	return fmt.Sprint(v)
}

// DocLenhStdin – đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0.
func DocLenhStdin(t0 time.Time, xuLy func(string), ketThuc func()) {
	sc := bufio.NewScanner(os.Stdin)
	for sc.Scan() {
		dong := strings.TrimSpace(sc.Text())
		if strings.HasPrefix(dong, "@") {
			moc, con, _ := strings.Cut(dong, " ")
			if t, err := strconv.ParseFloat(moc[1:], 64); err == nil {
				time.Sleep(time.Until(t0.Add(time.Duration(t * float64(time.Second)))))
			}
			dong = strings.TrimSpace(con)
		}
		if dong != "" {
			xuLy(dong)
		}
	}
	if ketThuc != nil {
		ketThuc()
	}
}

// DocThamSo – ["thoi_gian_s=5", "ten_mon=Bình"] -> TS{thoi_gian_s: 5, ten_mon: "Bình"} (số nếu đọc được).
func DocThamSo(ds []string) (TS, error) {
	var kq TS
	for _, muc := range ds {
		k, v, co := strings.Cut(muc, "=")
		if !co {
			return nil, fmt.Errorf("cần dạng khóa=giá_trị: %s", muc)
		}
		var gt any = v
		if n, err := strconv.ParseInt(strings.TrimSpace(v), 10, 64); err == nil {
			gt = n
		} else if f, err := strconv.ParseFloat(strings.TrimSpace(v), 64); err == nil {
			gt = f
		}
		kq = kq.Dat(k, gt)
	}
	return kq, nil
}

// EpKieu – ép giá trị v về kiểu của giá trị mặc định (int(v), float(v), str(v) của Python).
func EpKieu(macDinh, v any) (any, error) {
	switch macDinh.(type) {
	case int64:
		switch x := v.(type) {
		case int64:
			return x, nil
		case float64:
			return int64(x), nil
		case bool:
			return SoNguyen(x), nil
		case string:
			n, err := strconv.ParseInt(strings.TrimSpace(x), 10, 64)
			if err != nil {
				return nil, fmt.Errorf("invalid literal for int() with base 10: '%s'", x)
			}
			return n, nil
		}
		return nil, fmt.Errorf("int() argument must be a string or a real number, not '%s'", tenKieuPy(v))
	case float64:
		switch x := v.(type) {
		case int64:
			return float64(x), nil
		case float64:
			return x, nil
		case bool:
			return float64(SoNguyen(x)), nil
		case string:
			f, err := strconv.ParseFloat(strings.TrimSpace(x), 64)
			if err != nil {
				return nil, fmt.Errorf("could not convert string to float: '%s'", x)
			}
			return f, nil
		}
		return nil, fmt.Errorf("float() argument must be a string or a real number, not '%s'", tenKieuPy(v))
	case string:
		return StrPy(v), nil
	}
	return v, nil
}

func tenKieuPy(v any) string {
	switch v.(type) {
	case nil:
		return "NoneType"
	case []any:
		return "list"
	case M:
		return "dict"
	}
	return fmt.Sprintf("%T", v)
}

// ================================================================ Định dạng in (giống nhau ở 5 ngôn ngữ)

// Tien – 5300000 -> "5.300.000 đ" (dấu chấm ngăn nghìn).
func Tien(x int64) string {
	s := strconv.FormatInt(x, 10)
	dau := ""
	if strings.HasPrefix(s, "-") {
		dau, s = "-", s[1:]
	}
	var b strings.Builder
	for i, c := range s {
		if i > 0 && (len(s)-i)%3 == 0 {
			b.WriteByte('.')
		}
		b.WriteRune(c)
	}
	return dau + b.String() + " đ"
}

// FmtG – format(x, "g") của Python: 2.0 -> "2", 0.5 -> "0.5".
func FmtG(x float64) string { return strconv.FormatFloat(x, 'g', 6, 64) }

// SoUs – độ trễ µs (số nguyên) hoặc "–" khi chưa có mẫu.
func SoUs(v any) string {
	if v == nil {
		return "–"
	}
	return strconv.FormatInt(SoNguyen(v), 10)
}

// DSM – danh sách đối tượng trong thông điệp ([]any của JSON hoặc []M của máy chủ Go).
func DSM(v any) []M {
	switch x := v.(type) {
	case []M:
		return x
	case []any:
		ds := make([]M, 0, len(x))
		for _, p := range x {
			if m, ok := p.(M); ok {
				ds = append(ds, m)
			}
		}
		return ds
	}
	return nil
}

// DSChuoi – danh sách chuỗi trong thông điệp.
func DSChuoi(v any) []string {
	switch x := v.(type) {
	case []string:
		return x
	case []any:
		ds := make([]string, 0, len(x))
		for _, p := range x {
			ds = append(ds, StrPy(p))
		}
		return ds
	}
	return nil
}

// DongSaiPham – "Sai phạm 37: giá thấp hơn 36, giá cuối sai 1".
func DongSaiPham(tk M) string {
	var ds []string
	for _, s := range DSM(tk["sai_pham"]) {
		ds = append(ds, fmt.Sprintf("%s %d", Chuoi(s["ten"]), SoNguyen(s["so"])))
	}
	return fmt.Sprintf("Sai phạm %d: ", SoNguyen(tk["sai_pham_tong"])) + strings.Join(ds, ", ")
}

// DongThongKe – các dòng (≤ 64 ký tự) in số liệu máy chủ – dùng ở server, khach, mo_phong.
func DongThongKe(d M) []string {
	return []string{
		fmt.Sprintf("Chế độ %s, trễ xử lý 0–%s ms", Chuoi(d["che_do"]), FmtG(SoThuc(d["tre_ms"]))),
		fmt.Sprintf("Yêu cầu %d: thành công %d, từ chối %d", SoNguyen(d["so_yc"]), SoNguyen(d["thanh_cong"]),
			SoNguyen(d["tu_choi"])),
		fmt.Sprintf("Xung đột %d, thử lại %d, hết lượt thử %d", SoNguyen(d["xung_dot"]), SoNguyen(d["thu_lai"]),
			SoNguyen(d["het_lan_thu"])),
		fmt.Sprintf("Hàng đợi max %d, chờ khóa quá hạn %d", SoNguyen(d["hang_doi_max"]), SoNguyen(d["khoa_cho_lau"])),
		fmt.Sprintf("Xử lý µs: p50 %s p95 %s p99 %s", SoUs(d["p50_us"]), SoUs(d["p95_us"]), SoUs(d["p99_us"])),
		fmt.Sprintf("Thông lượng %d yc/s", SoNguyen(d["thong_luong"])),
		DongSaiPham(d),
		Chuoi(d["tom_tat"]),
	}
}

// HangSoSanh – một hàng của bảng so sánh 4 chế độ.
type HangSoSanh struct {
	CheDo                                    string
	ThanhCong, SaiPham, XungDot, YcS, TuChoi int64
	P99Ms                                    *float64
	TomTat                                   string
}

const TieuDeSoSanh = "  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms"

// BangSoSanh – các dòng bảng so sánh.
func BangSoSanh(hang []HangSoSanh) []string {
	ds := []string{TieuDeSoSanh}
	for _, h := range hang {
		p99 := "–"
		if h.P99Ms != nil {
			p99 = fmt.Sprintf("%.1f", *h.P99Ms)
		}
		ds = append(ds, fmt.Sprintf("  %-12s %10d %8d %8d %5d %7s", h.CheDo, h.ThanhCong, h.SaiPham, h.XungDot,
			h.YcS, p99))
	}
	return ds
}

// LamTron – round() của Python (làm tròn nửa về số chẵn).
func LamTron(x float64) int64 { return int64(math.RoundToEven(x)) }
