// Bài mở rộng MR5.4 – Nhật ký theo luồng (tên luồng + biến cục bộ luồng ThreadLocal<T>).
//
// Mỗi dòng: "[  0.123 s] tải-1 yc=1      | nội dung"
// - Thread.CurrentThread.Name cho biết LUỒNG NÀO làm việc đó;
// - ngữ cảnh "yc=..." nằm trong ThreadLocal<string>: mỗi luồng có bản sao riêng, đặt một lần rồi mọi lời
//   gọi Log() sau đó tự có (với mã async/await, ngữ cảnh đi theo tác vụ thì dùng AsyncLocal<T>).
// Một khóa bảo đảm hai luồng không in xen lẫn trên một dòng.
using System.Diagnostics;

namespace BaiMR5_4;

static class NhatKy
{
    static readonly Stopwatch Dong = Stopwatch.StartNew();
    static readonly ThreadLocal<string> NguCanh = new(() => "");   // biến cục bộ luồng
    static readonly object KhoaIn = new();

    public static void DatNguCanh(string s) => NguCanh.Value = s;

    public static double BayGio() => Dong.Elapsed.TotalSeconds;

    public static void Log(string msg)
    {
        string ai = ((Thread.CurrentThread.Name ?? "?") + " " + NguCanh.Value).Trim();
        lock (KhoaIn) Console.WriteLine($"[{BayGio(),7:F3} s] {ai,-16}| {msg}");
    }

    public static void Say(string msg = "")
    {
        lock (KhoaIn) Console.WriteLine(msg);
    }
}
