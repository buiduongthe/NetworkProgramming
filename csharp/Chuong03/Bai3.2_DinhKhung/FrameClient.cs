using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json.Nodes;

namespace Bai3_2;

// Mã giả 3.2 với vòng gửi nhận thô thay bằng SEND_JSON/RECV_JSON (Mã giả 3.6).
// Tham số thứ ba (tùy chọn): số ký tự của thông điệp lớn gửi sau khi hết dữ liệu nhập, ví dụ 200000
static class FrameClient
{
    const string Pattern = "Mạng máy tính ";                           // 14 ký tự, 18 byte UTF-8

    static string MakeText(int n)                                       // chuỗi đúng n ký tự có chữ tiếng Việt
    {
        var sb = new StringBuilder();
        while (sb.Length < n) sb.Append(Pattern);
        return sb.ToString(0, n);
    }

    static string Sha256Hex(string text) =>
        Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(text)));

    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args.Take(2).ToArray(), "127.0.0.1", 9000);
        int big = args.Length > 2 ? int.Parse(args[2]) : 0;
        using Socket s = NetUtil.SafeConnect(host, port, 3000);
        s.ReceiveTimeout = 5000;                                        // RECV_TIMEOUT 5000 ms
        Console.WriteLine($"[client] Đã kết nối {host}:{port}. Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).");
        string? line;
        while ((line = Console.ReadLine()) != null)                     // READ_CONSOLE()
        {
            Framing.SendJson(s, new JsonObject { ["text"] = line });   // SEND_JSON thay cho SEND_ALL thô
            var resp = Framing.RecvJson(s);                             // RECV_JSON: luôn nhận đủ một khung
            Console.WriteLine($"[client] Gửi: '{line}' -> Nhận: '{resp?["text"]}'");
        }
        if (big > 0)
        {
            string text = MakeText(big);
            Console.WriteLine($"[client] Gửi thông điệp lớn: {text.Length} ký tự, {Encoding.UTF8.GetByteCount(text)} byte UTF-8");
            Framing.SendJson(s, new JsonObject { ["text"] = text });
            string back = Framing.RecvJson(s)?["text"]?.GetValue<string>() ?? "";
            bool same = back == text;
            Console.WriteLine($"[client] Nhận lại: {back.Length} ký tự, {Encoding.UTF8.GetByteCount(back)} byte; trùng khớp: {(same ? "có" : "không")}");
            Console.WriteLine($"[client] SHA-256 gửi:  {Sha256Hex(text)[..32]}…");
            Console.WriteLine($"[client] SHA-256 nhận: {Sha256Hex(back)[..32]}…");
        }
        s.Shutdown(SocketShutdown.Send);                                // SHUTDOWN(WRITE)
        Console.WriteLine("[client] Đã đóng kết nối");                  // 'using' đóng s (CLOSE)
        return 0;
    }
}
