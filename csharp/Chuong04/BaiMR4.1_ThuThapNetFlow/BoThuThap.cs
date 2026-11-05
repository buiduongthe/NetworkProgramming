using System.Globalization;
using System.Net;
using System.Net.Sockets;

namespace BaiMR4_1;

// Bộ thu thập NetFlow v5 (collector) trên UDP.
// Server UDP nhận datagram, mỗi datagram là trọn một gói xuất (UDP giữ ranh giới thông điệp),
// giải mã nhị phân bằng BinaryPrimitives như Bài 4.3 (truy vấn DNS). Bộ thu:
//   - kiểm tra tính hợp lệ (phiên bản, độ dài = 24 + 48 x count) và loại gói hỏng;
//   - phát hiện mất gói nhờ trường flow_sequence (giống số thứ tự ở Bài 4.4);
//   - tổng hợp: nguồn dùng nhiều băng thông nhất, dịch vụ (cổng/giao thức) phổ biến.
// Dừng khi nhàn rỗi quá --nhan-roi giây sau gói đầu tiên (hoặc Ctrl+C) và in báo cáo.
static class BoThuThap
{
    public static int Run(string[] args)
    {
        var pos = new List<string>();
        double idle = 1.5;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--nhan-roi") idle = double.Parse(args[++i], CultureInfo.InvariantCulture);
            else pos.Add(args[i]);
        }
        string host = pos.Count > 0 ? pos[0] : "127.0.0.1";
        int port = pos.Count > 1 ? int.Parse(pos[1]) : 2055;
        // in số giây giống Python: 1.5 -> "1.5", 2 -> "2.0"
        string idleText = idle == Math.Floor(idle) ? idle.ToString("F1", CultureInfo.InvariantCulture)
                                                   : idle.ToString(CultureInfo.InvariantCulture);

        var sock = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        sock.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        Console.WriteLine($"[thu-thap] Nghe NetFlow v5 tại UDP {host}:{port}");
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; sock.Close(); };   // Ctrl+C: dừng nhận, vẫn in báo cáo

        var bySrc = new Dictionary<string, long>();
        var byService = new Dictionary<(int Port, int Proto), (long N, long Bytes)>();
        var expected = new Dictionary<(string, byte), uint>();   // (địa chỉ máy xuất, engine) -> seq mong đợi
        int ok = 0, bad = 0;
        long flows = 0, lost = 0, totalBytes = 0;
        var buf = new byte[2048];                                // gói v5 lớn nhất 24 + 30*48 = 1464 byte
        while (true)
        {
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try
            {
                n = sock.ReceiveFrom(buf, ref from);
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut)
            {
                Console.WriteLine($"[thu-thap] Nhàn rỗi {idleText} s, kết thúc");
                break;
            }
            catch (Exception e) when (e is SocketException || e is ObjectDisposedException)
            {
                break;                                           // socket đã đóng do Ctrl+C
            }
            sock.ReceiveTimeout = (int)(idle * 1000);            // sau gói đầu tiên mới bắt đầu đếm nhàn rỗi
            var addr = (IPEndPoint)from;
            Header head;
            List<Flow> records;
            try
            {
                (head, records) = NetflowV5.Parse(buf.AsSpan(0, n));
            }
            catch (FormatException e)
            {
                bad++;
                Console.WriteLine($"[thu-thap] Loại gói từ {addr.Address}:{addr.Port}: {e.Message}");
                continue;
            }
            var key = (addr.Address.ToString(), head.EngineId);
            if (expected.TryGetValue(key, out uint want) && head.Seq != want)
            {
                long gap = (long)head.Seq - want;
                lost += Math.Max(gap, 0);
                Console.WriteLine($"[thu-thap] MẤT GÓI: chờ seq={want}, nhận seq={head.Seq}, thiếu {gap} luồng");
            }
            expected[key] = head.Seq + (uint)head.Count;
            ok++;
            flows += head.Count;
            Console.WriteLine($"[thu-thap] Gói seq={head.Seq}: {head.Count} luồng");
            foreach (var r in records)
            {
                bySrc[r.Src] = bySrc.GetValueOrDefault(r.Src) + r.Octets;
                var svc = byService.GetValueOrDefault((r.Dport, r.Proto));
                byService[(r.Dport, r.Proto)] = (svc.N + 1, svc.Bytes + r.Octets);
                totalBytes += r.Octets;
            }
        }
        sock.Close();
        Console.WriteLine($"[thu-thap] Hợp lệ {ok} gói, loại {bad} gói, mất {lost} luồng");
        Console.WriteLine($"[thu-thap] Đã thu {flows} luồng, tổng {totalBytes} byte");
        if (totalBytes == 0) return 0;
        Console.WriteLine("[thu-thap] Nguồn dùng nhiều byte nhất:");
        foreach (var (src, b) in bySrc.OrderBy(kv => -kv.Value).ThenBy(kv => kv.Key, StringComparer.Ordinal).Take(3))
            Console.WriteLine($"  {src,-12} {b,9} byte ({(b * 100.0 / totalBytes).ToString("F1", CultureInfo.InvariantCulture)}%)");
        Console.WriteLine("[thu-thap] Dịch vụ theo số luồng:");
        foreach (var ((p, proto), (cnt, b)) in byService.OrderBy(kv => -kv.Value.N).ThenBy(kv => kv.Key.Port).ThenBy(kv => kv.Key.Proto))
        {
            string name = $"{p}/{NetflowV5.ProtoName.GetValueOrDefault(proto, proto.ToString())}";
            Console.WriteLine($"  {name,-9} {cnt,3} luồng {b,9} byte");
        }
        return 0;
    }
}
