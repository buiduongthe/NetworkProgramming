using System.Globalization;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using static BaiMR6_3.Chung;

namespace BaiMR6_3;

// Client phòng hội thoại video.
// Vòng gửi: theo tốc độ khung cho phép (min(fps camera, fps tối đa server đặt)), lấy một khung từ camera
// tổng hợp, giảm độ phân giải theo yêu cầu của server, nén zlib, chia mảnh ≤ 1200 byte, gửi UDP.
// Luồng nhận: ghép mảnh thành khung lưới do server gửi, giải nén, giữ khung mới nhất.
// Mỗi giây in fps và kbit/s gửi/nhận, số mảnh mất. Lệnh `chup <nhãn>` lưu khung lưới đang xem thành PNG
// (out/chup_N.png); khi kết thúc, --ghep tệp.png ghép mọi ảnh đã chụp kèm nhãn.
// Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi client chạy):
//   tat_camera   bat_camera   nhan_fps <n>   nhan_do_phan_giai <1|2>   chup <nhãn>   thoat
static class KhachVideo
{
    /// <summary>Ghép các ảnh chụp (rộng, cao, dữ liệu RGB, nhãn) thành một ảnh: 2 cột, nhãn dưới mỗi ảnh.</summary>
    static (int W, int H) GhepAnh(string path, List<(int W, int H, byte[] Anh, string Nhan)> ds)
    {
        const int cot = 2;
        int oW = ds.Max(d => d.W) + 16;
        var caoHang = new List<int>();
        for (int i = 0; i < ds.Count; i += cot) caoHang.Add(ds.Skip(i).Take(cot).Max(d => d.H) + 34);
        int W = cot * oW + 16, H = caoHang.Sum() + 8;
        var anh = Enumerable.Repeat((byte)0xFF, W * H * 3).ToArray();
        for (int n = 0; n < ds.Count; n++)
        {
            var (w, h, duLieu, nhan) = ds[n];
            int x0 = 16 + (n % cot) * oW + (oW - 16 - w) / 2;
            int y0 = 8 + caoHang.Take(n / cot).Sum();
            for (int y = 0; y < h; y++) Array.Copy(duLieu, y * w * 3, anh, ((y0 + y) * W + x0) * 3, w * 3);
            VeChu(anh, W, H, 16 + (n % cot) * oW, y0 + h + 6, nhan.Length > 52 ? nhan[..52] : nhan, 1, (0, 0, 0));
        }
        GhiPng(path, anh, W, H);
        return (W, H);
    }

    public static int Run(string[] args)
    {
        // <tên> <host> <cổng_tcp> [--fps 10] [--thoi-gian 10] [--mat-goi 0.02] [--ghep ket_qua_video.png]
        var pos = new List<string>();
        var o = new Dictionary<string, string>();
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i].StartsWith("--") && i + 1 < args.Length) o[args[i]] = args[++i];
            else pos.Add(args[i]);
        }
        double D(string k, double d) => o.TryGetValue(k, out var v) ? double.Parse(v, CultureInfo.InvariantCulture) : d;
        string ten = pos[0], host = pos[1];
        int cong = int.Parse(pos[2]);
        int fpsCam = (int)D("--fps", 10);
        double thoiGian = D("--thoi-gian", 10), matGoi = D("--mat-goi", 0);
        string? ghepTep = o.GetValueOrDefault("--ghep");
        string tag = $"[khach {ten}]";
        void Log(string text) => Console.WriteLine($"{tag} {text}");

        // ---- Kênh điều khiển TCP
        var tcp = new Socket(SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };
        tcp.Connect(host, cong);
        var f = new StreamReader(new NetworkStream(tcp), Encoding.UTF8);
        GuiJson(tcp, new JsonObject { ["loai"] = "vao", ["ten"] = ten });
        var chao = JsonNode.Parse(f.ReadLine()!)!.AsObject();
        ushort ma = chao["ma"]!.GetValue<ushort>();
        var camera = new CameraTongHop(ten, ma);
        Log($"Vào phòng, mã {ma}; camera tổng hợp {Rong}x{Cao}, {fpsCam} fps");

        var udp = new Socket(SocketType.Dgram, ProtocolType.Udp) { ReceiveBufferSize = 1 << 20 };
        udp.Connect(host, chao["cong_udp"]!.GetValue<int>());      // chỉ nhận datagram từ server

        var khoa = new object();
        bool mo = false, dung = false;
        int fpsCho = fpsCam, chia = 1;
        var ghep = new GhepKhung();
        (int W, int H, byte[] Anh)? xem = null;                    // khung lưới mới nhất
        int khungNhan = 0, byteNhan = 0;
        var anhChup = new List<(int W, int H, byte[] Anh, string Nhan)>();
        double t0 = Now;

        void DocTcp()
        {
            try
            {
                string? dong;
                while ((dong = f.ReadLine()) != null)
                {
                    var tin = JsonNode.Parse(dong)!.AsObject();
                    switch (tin["loai"]?.GetValue<string>())
                    {
                        case "camera":
                            lock (khoa)
                            {
                                mo = tin["mo"]!.GetValue<bool>();
                                fpsCho = tin["fps"]!.GetValue<int>();
                                chia = tin["chia"]!.GetValue<int>();
                            }
                            Log($"Camera: {tin["ly_do"]}, gửi tối đa {tin["fps"]} fps, 1/{tin["chia"]}");
                            break;
                        case "nhan": Log($"Nhận: {tin["fps"]} fps, độ phân giải 1/{tin["chia"]}"); break;
                        case "su_kien": Log($"Sự kiện: {tin["noi_dung"]}"); break;
                        case "tu_choi": Log($"Bị từ chối: {tin["ly_do"]}"); break;
                    }
                }
            }
            catch (Exception e) when (e is IOException || e is SocketException || e is JsonException || e is ObjectDisposedException) { }
            lock (khoa) dung = true;
        }

        void NhanUdp()
        {
            var buf = new byte[2048];
            while (true)
            {
                int n;
                try { n = udp.Receive(buf); }
                catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { return; }
                var h = TachHeader(buf.AsSpan(0, n));
                if (h == null || h.Loai != LoaiManh) continue;
                (int W, int H, byte[] DuLieu)? khung;
                lock (khoa) khung = ghep.Them(h.So, h.I, h.Tong, h.W, h.H, h.Payload, Now);
                if (khung is not { } k) continue;
                byte[] anh;
                try { anh = GiaiNen(k.DuLieu); }
                catch (InvalidDataException) { continue; }
                lock (khoa)
                {
                    xem = (h.W, h.H, anh);
                    khungNhan++;
                    byteNhan += k.DuLieu.Length;
                }
            }
        }

        void Lenh(string dong)
        {
            var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            Log($"> {dong}");
            switch (p[0])
            {
                case "tat_camera":                         // tự tắt: có hiệu lực ngay
                    lock (khoa) mo = false;
                    GuiJson(tcp, new JsonObject { ["loai"] = "tu_tat_camera" });
                    break;
                case "bat_camera":                         // tự bật: chờ server cho phép
                    GuiJson(tcp, new JsonObject { ["loai"] = "tu_bat_camera" });
                    break;
                case "nhan_fps" when p.Length == 2:
                    GuiJson(tcp, new JsonObject { ["loai"] = "nhan", ["fps"] = int.Parse(p[1]) });
                    break;
                case "nhan_do_phan_giai" when p.Length == 2:
                    GuiJson(tcp, new JsonObject { ["loai"] = "nhan", ["chia"] = int.Parse(p[1]) });
                    break;
                case "chup":
                    (int W, int H, byte[] Anh)? a;
                    lock (khoa) a = xem;
                    if (a is not { } x)
                    {
                        Log("chưa có hình để chụp");
                        return;
                    }
                    Directory.CreateDirectory("out");
                    string duong = $"out/chup_{anhChup.Count + 1}.png";
                    GhiPng(duong, x.Anh, x.W, x.H);
                    string nhan = p.Length > 1 ? string.Join(" ", p[1..]) : duong;
                    anhChup.Add((x.W, x.H, x.Anh, nhan));
                    Log($"Đã chụp {duong} ({x.W}x{x.H})");
                    break;
                case "thoat":
                    lock (khoa) dung = true;
                    break;
                default:
                    Log("lệnh không hợp lệ");
                    break;
            }
        }

        void DocLenh()
        {
            string? dong;
            while ((dong = Console.ReadLine()) != null)
            {
                dong = dong.Split('#')[0].Trim();
                if (dong.StartsWith('@'))
                {
                    int sp = dong.IndexOf(' ');
                    string mocT = sp < 0 ? dong : dong[..sp];
                    dong = sp < 0 ? "" : dong[(sp + 1)..];
                    double cho = double.Parse(mocT[1..], CultureInfo.InvariantCulture) - (Now - t0);
                    if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
                }
                if (dong != "") Lenh(dong);
            }
        }

        foreach (var h in new Action[] { DocTcp, NhanUdp, DocLenh })
            new Thread(() => h()) { IsBackground = true }.Start();

        // ---- Vòng gửi khung theo fps cho phép; mỗi giây in thống kê
        var rng = new Random(ma);
        udp.Send(GoiGiu(ma));                            // đăng ký địa chỉ UDP với server
        long soKhung = 0;
        int guiKhung = 0, guiManh = 0, boGiaLap = 0, demGui = 0, byteGui = 0, giay = 0;
        double hanGui = Now, mocDo = hanGui, hanGiu = mocDo + 1.0;
        Log("Mỗi giây: fps và kbit/s gửi | nhận, số mảnh mất");
        while (Now - t0 < thoiGian)
        {
            double bayGio = Now;
            bool m;
            int fps, c;
            lock (khoa)
            {
                if (dung) break;
                (m, fps, c) = (mo, Math.Min(fpsCam, fpsCho), chia);
            }
            if (m && bayGio >= hanGui)
            {
                hanGui = Math.Max(hanGui + 1.0 / fps, bayGio);
                var (anh, w, h) = ThuNho(camera.LayKhung(soKhung), Rong, Cao, c);   // giảm độ phân giải trước khi nén
                var nen = Nen(anh);
                foreach (var g in ChiaManh(ma, soKhung, w, h, nen))
                {
                    guiManh++;
                    if (rng.NextDouble() < matGoi) boGiaLap++;    // "mạng" làm rơi mảnh
                    else udp.Send(g);
                }
                soKhung++;
                guiKhung++;
                demGui++;
                byteGui += nen.Length;
            }
            else if (!m && bayGio >= hanGiu)
            {
                udp.Send(GoiGiu(ma));                     // giữ kết nối khi tắt camera
                hanGiu = bayGio + 1.0;
            }
            if (bayGio - mocDo >= 1.0)
            {
                mocDo += 1.0;
                giay++;
                int kn, bn, mat;
                lock (khoa)
                {
                    (kn, bn, mat) = (khungNhan, byteNhan, ghep.ManhMat);
                    khungNhan = byteNhan = 0;
                }
                Log($"{giay,2}s gửi {demGui,2} fps {F(byteGui * 8 / 1000.0, "F0"),4} | nhận {kn,2} fps {F(bn * 8 / 1000.0, "F0"),5}, mất {mat}");
                demGui = byteGui = 0;
            }
            Thread.Sleep(2);
        }

        // ---- Kết thúc
        double tong = Now - t0;
        tcp.Close();
        udp.Close();
        Log($"Kết thúc sau {F(tong, "F1")} s");
        Log($"Gửi {guiKhung} khung = {guiManh} mảnh, giả lập mất {boGiaLap} mảnh");
        Log($"Nhận {ghep.KhungDu} khung đủ, hỏng {ghep.KhungHong}, mất hẳn {ghep.KhungMatHan}, mảnh mất {ghep.ManhMat}");
        if (ghepTep != null && anhChup.Count > 0)
        {
            var (W, H) = GhepAnh(ghepTep, anhChup);
            Log($"Đã ghép {anhChup.Count} ảnh chụp vào {ghepTep} ({W}x{H})");
        }
        return 0;
    }
}
