// Bài mở rộng MR5.4 – Dịch vụ thử (máy chủ phụ trợ) cho các vai trò luong, hang-doi.
// Giao thức giống bản Python/Go, mỗi kết nối một lệnh, mỗi kết nối một luồng:
//   VIEC <mã> <ms>  → chờ <ms> rồi trả "XONG <mã>"
//   PHAT <n>        → gửi n dòng "GOI <k> <dữ liệu>", đọc n dòng "KQ <k> <tổng byte>", trả "NHAN <đúng>/<n>"
//   THONGKE         → trả "TK <số yêu cầu VIEC> <đồng thời tối đa>", in tóm tắt "đợt", đặt lại
using System.Net;
using System.Net.Sockets;
using System.Text;
using static BaiMR5_4.NhatKy;

namespace BaiMR5_4;

static class DichVu
{
    public static readonly UTF8Encoding Utf8 = new(false);

    // Dữ liệu của gói thứ k (giống nhau ở mọi bản ngôn ngữ)
    public static string DuLieu(int k) => string.Concat(Enumerable.Repeat($"goi-so-{k}-", 3));

    // Kết quả xử lý một gói: tổng giá trị các byte UTF-8, lấy dư 65536
    public static int TongByte(string s) => Utf8.GetBytes(s).Sum(b => (int)b) % 65536;

    public static (StreamReader, StreamWriter) Luong(TcpClient c)
    {
        var ns = c.GetStream();
        return (new StreamReader(ns, Utf8), new StreamWriter(ns, Utf8) { NewLine = "\n", AutoFlush = true });
    }

    static readonly object Khoa = new();
    static int tong, dangChay, toiDa;

    static void PhucVu(TcpClient c)
    {
        using (c)
        {
            var (r, w) = Luong(c);
            var p = (r.ReadLine() ?? "").Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (p.Length == 0) return;
            switch (p[0])
            {
                case "VIEC":
                    lock (Khoa) { tong++; dangChay++; toiDa = Math.Max(toiDa, dangChay); }
                    Thread.Sleep(int.Parse(p[2]));
                    lock (Khoa) dangChay--;
                    w.WriteLine($"XONG {p[1]}");
                    break;
                case "PHAT":
                    int n = int.Parse(p[1]);
                    w.AutoFlush = false;
                    for (int k = 1; k <= n; k++) w.WriteLine($"GOI {k} {DuLieu(k)}");
                    w.Flush();
                    w.AutoFlush = true;
                    int dung = 0;
                    for (int i = 0; i < n; i++)
                    {
                        var kq = (r.ReadLine() ?? "").Split(' ');
                        if (kq.Length == 3 && int.Parse(kq[2]) == TongByte(DuLieu(int.Parse(kq[1])))) dung++;
                    }
                    w.WriteLine($"NHAN {dung}/{n}");
                    Say($"[dịch vụ] PHAT {n}: nhận {n} kết quả, đúng {dung}/{n}");
                    break;
                case "THONGKE":
                    int a, b;
                    lock (Khoa) { a = tong; b = toiDa; tong = toiDa = 0; }
                    w.WriteLine($"TK {a} {b}");
                    Say($"[dịch vụ] đợt: {a} yêu cầu VIEC, đồng thời tối đa {b}");
                    break;
            }
        }
    }

    public static void Run(string host, int port)
    {
        var ln = new TcpListener(IPAddress.Parse(host), port);
        ln.Start(256);
        Say($"[dịch vụ] lắng nghe {host}:{port} (mỗi kết nối một luồng)");
        while (true)
        {
            var c = ln.AcceptTcpClient();
            new Thread(() => PhucVu(c)) { IsBackground = true }.Start();
        }
    }
}
