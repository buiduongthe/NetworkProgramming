using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace BaiMR5_1;

// Server truyền nhiều tệp: mỗi kết nối phục vụ một lượt tải xuống/tải lên.
// Giao thức: nhận một khung JSON điều khiển, rồi truyền luồng byte (xem Khung.cs).
//   - download: {"op":"download","name":TÊN,"offset":OFF}
//        -> trả {"ok":true,"size":TỔNG,"sha256":HEX} rồi gửi byte [OFF..hết].
//   - upload:   {"op":"upload","name":TÊN,"offset":OFF,"size":TỔNG,"sha256":HEX}
//        -> trả {"ok":true} rồi nhận byte, ghi nối vào TÊN.part từ OFF; xong thì kiểm SHA-256,
//           đổi tên thành TÊN, trả {"ok":true,"sha256_ok":...}.
// Server KHÔNG tự giới hạn tốc độ – client điều tiết, TCP tự tạo áp lực ngược.
static class TruyenServer
{
    public static void PhucVu(Socket conn, string kho)
    {
        try
        {
            var req = Khung.NhanKhung(conn);
            string? op = req["op"]?.GetValue<string>();
            string name = Path.GetFileName(req["name"]?.GetValue<string>() ?? "");   // chặn "../" trong tên
            long off = req["offset"]?.GetValue<long>() ?? 0;
            if (op == "download")
            {
                string path = Path.Combine(kho, name);
                if (name == "" || !File.Exists(path))
                {
                    Khung.GuiKhung(conn, new JsonObject { ["ok"] = false, ["loi"] = "Không có tệp " + name });
                    return;
                }
                long size = new FileInfo(path).Length;
                Khung.GuiKhung(conn, new JsonObject { ["ok"] = true, ["size"] = size, ["sha256"] = Khung.Sha256Tep(path) });
                using var f = File.OpenRead(path);
                f.Seek(off, SeekOrigin.Begin);
                var buf = new byte[Khung.Chunk];
                int n;
                while ((n = f.Read(buf, 0, buf.Length)) > 0) Khung.SendAll(conn, buf, n);
            }
            else if (op == "upload")
            {
                long total = req["size"]?.GetValue<long>() ?? 0;
                string part = Path.Combine(kho, name + ".part");
                Khung.GuiKhung(conn, new JsonObject { ["ok"] = true });
                long nhan = off;
                using (var f = new FileStream(part, off > 0 && File.Exists(part) ? FileMode.OpenOrCreate : FileMode.Create))
                {
                    f.Seek(off, SeekOrigin.Begin);
                    while (nhan < total)
                    {
                        byte[] b = Khung.DocDu(conn, (int)Math.Min(Khung.Chunk, total - nhan));
                        f.Write(b);
                        nhan += b.Length;
                    }
                }
                bool shaOk = Khung.Sha256Tep(part) == req["sha256"]?.GetValue<string>();
                if (shaOk) File.Move(part, Path.Combine(kho, name), overwrite: true);
                Khung.GuiKhung(conn, new JsonObject { ["ok"] = true, ["sha256_ok"] = shaOk });
            }
            else Khung.GuiKhung(conn, new JsonObject { ["ok"] = false, ["loi"] = "op lạ" });
        }
        catch (Exception e) when (e is IOException || e is SocketException || e is ObjectDisposedException) { }
        // client ngắt giữa chừng (pause/stop) là bình thường
        finally
        {
            conn.Close();
        }
    }

    /// <summary>Vòng chấp nhận kết nối: mỗi kết nối một luồng phục vụ.</summary>
    public static void VongServer(TcpListener srv, string kho)
    {
        while (true)
        {
            Socket conn;
            try { conn = srv.AcceptSocket(); }
            catch (Exception) { return; }          // socket nghe đã đóng
            new Thread(() => PhucVu(conn, kho)) { IsBackground = true }.Start();
        }
    }

    public static int Run(string[] a)
    {
        if (a.Length < 2)
        {
            Console.WriteLine("Cách dùng: dotnet run -- server <host> <port> [thư_mục_lưu]");
            return 2;
        }
        string host = a[0], kho = a.Length > 2 ? a[2] : "kho_server";
        int port = int.Parse(a[1]);
        Directory.CreateDirectory(kho);
        var srv = new TcpListener(IPAddress.Parse(host), port);
        srv.Start(16);
        Console.WriteLine($"[server] Nghe tại {host}:{port}, thư mục lưu: {kho}");
        var done = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; srv.Stop(); done.Set(); };
        new Thread(() => VongServer(srv, kho)) { IsBackground = true }.Start();
        done.Wait();
        Console.WriteLine("[server] Dừng.");
        return 0;
    }
}
