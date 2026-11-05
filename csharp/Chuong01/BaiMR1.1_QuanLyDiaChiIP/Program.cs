// Bài mở rộng MR1.1 – Quản lý địa chỉ IP (IPAM): chia mạng con VLSM, khối còn trống,
// kiểm tra chồng lấn, cấp phát địa chỉ.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Mục 1.2.3 (địa chỉ IP, tiền tố mạng)
// và Bài 1.1. Chương trình đọc một kế hoạch địa chỉ (tệp JSON) và:
//   1. Chia khối địa chỉ thành các mạng con có kích thước khác nhau (VLSM):
//      yêu cầu lớn được cấp trước để các khối luôn thẳng hàng (aligned).
//   2. Liệt kê phần địa chỉ còn trống dưới dạng các khối CIDR.
//   3. Kiểm tra các tiền tố đề xuất: sai cú pháp, có bit máy, nằm ngoài khối, chồng lấn.
//   4. Cấp phát địa chỉ cho thiết bị trong một mạng con (bỏ qua địa chỉ cổng mặc định).
// Chỉ xử lý IPv4 cho gọn: địa chỉ được đổi sang số nguyên 32 bit (uint) để tính khối.
//
// Chạy:  dotnet run -- [tệp_kế_hoạch.json ...]      (mặc định ke_hoach.json)
using System.Globalization;
using System.Text;
using System.Text.Json;
using BaiMR1_1;

Console.OutputEncoding = Encoding.UTF8;
CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // dấu chấm thập phân như Python
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;

string[] paths = args.Length > 0 ? args : ["ke_hoach.json"];
bool allOk = true;
foreach (var p in paths)
    allOk &= Ipam.RunPlan(p);          // chạy hết mọi kế hoạch, không dừng ở kế hoạch lỗi
return allOk ? 0 : 1;

namespace BaiMR1_1
{
    /// <summary>Một mạng IPv4: địa chỉ mạng (số 32 bit) và độ dài tiền tố.</summary>
    readonly record struct Net4(uint Addr, int Len)
    {
        public long Size => 1L << (32 - Len);                       // số địa chỉ của khối
        public uint Last => (uint)(Addr + Size - 1);                // địa chỉ quảng bá
        public uint Mask => Len == 0 ? 0 : uint.MaxValue << (32 - Len);
        public bool Overlaps(Net4 o) => Addr <= o.Last && o.Addr <= Last;
        public bool SubnetOf(Net4 o) => Addr >= o.Addr && Last <= o.Last;
        public override string ToString() => $"{Ipam.Str(Addr)}/{Len}";
    }

    static class Ipam
    {
        public static string Str(uint a) => $"{a >> 24}.{(a >> 16) & 255}.{(a >> 8) & 255}.{a & 255}";

        /// <summary>Phân tích "a.b.c.d" chặt chẽ: đúng 4 phần, mỗi phần 0–255, không có số 0 đứng đầu.</summary>
        static bool TryParseAddr(string s, out uint addr)
        {
            addr = 0;
            var parts = s.Split('.');
            if (parts.Length != 4) return false;
            foreach (var part in parts)
            {
                if (part.Length == 0 || part.Length > 3 || !part.All(char.IsAsciiDigit)) return false;
                if (part.Length > 1 && part[0] == '0') return false;
                int v = int.Parse(part);
                if (v > 255) return false;
                addr = (addr << 8) | (uint)v;
            }
            return true;
        }

        /// <summary>Phân tích "địa_chỉ/tiền_tố". strict=true: báo lỗi nếu còn bit máy; false: tự xóa bit máy.</summary>
        static bool TryParseNet(string text, bool strict, out Net4 net)
        {
            net = default;
            var parts = text.Split('/');
            if (parts.Length != 2 || !TryParseAddr(parts[0], out uint a)) return false;
            if (!int.TryParse(parts[1], NumberStyles.None, null, out int len) || len > 32) return false;
            var n = new Net4(0, len);
            uint masked = a & n.Mask;
            if (strict && masked != a) return false;        // còn bit máy (host bits)
            net = n with { Addr = masked };
            return true;
        }

        static Net4 ParseNet(string text) =>
            TryParseNet(text, true, out var n) ? n : throw new FormatException($"Tiền tố không hợp lệ: {text}");

        static bool IsPrivate(Net4 n) =>          // ba dải riêng của RFC 1918
            n.SubnetOf(ParseNet("10.0.0.0/8")) || n.SubnetOf(ParseNet("172.16.0.0/12")) ||
            n.SubnetOf(ParseNet("192.168.0.0/16"));

        /// <summary>Độ dài tiền tố nhỏ nhất chứa được nHosts máy (+2 địa chỉ mạng và quảng bá).</summary>
        static int PrefixForHosts(int nHosts)
        {
            long size = 4;                        // khối nhỏ nhất dùng ở đây là /30
            while (size - 2 < nHosts) size *= 2;
            return 32 - (int)Math.Log2(size);
        }

        record Alloc(string Name, int Hosts, Net4 Net);
        record Request(string Name, int Hosts);

        /// <summary>Cấp mạng con cho từng yêu cầu, lớn trước nhỏ sau.</summary>
        static List<Alloc> PlanVlsm(Net4 block, List<Request> requests)
        {
            var order = requests.OrderByDescending(r => r.Hosts).ToList();   // sắp xếp ổn định như sorted()
            long cursor = block.Addr;
            long end = block.Last;
            var result = new List<Alloc>();
            foreach (var r in order)
            {
                int plen = PrefixForHosts(r.Hosts);
                long size = 1L << (32 - plen);
                cursor = (cursor + size - 1) / size * size;                    // làm tròn lên bội của kích thước khối
                if (cursor + size - 1 > end)
                    throw new InvalidOperationException(
                        $"Không đủ chỗ cho {r.Name} ({r.Hosts} máy cần /{plen}) trong {block}");
                result.Add(new Alloc(r.Name, r.Hosts, new Net4((uint)cursor, plen)));
                cursor += size;
            }
            return result;
        }

        /// <summary>Tách dải [start, end] thành ít khối CIDR nhất (mỗi khối thẳng hàng theo kích thước).</summary>
        static IEnumerable<Net4> RangeToCidr(long start, long end)
        {
            while (start <= end)
            {
                int len = 32;
                // nới khối khi địa chỉ đầu còn thẳng hàng và khối lớn hơn vẫn nằm gọn trong dải
                while (len > 0 && start % (1L << (33 - len)) == 0 && start + (1L << (33 - len)) - 1 <= end)
                    len--;
                yield return new Net4((uint)start, len);
                start += 1L << (32 - len);
            }
        }

        /// <summary>Các khối CIDR còn trống trong block sau khi trừ các mạng đã dùng.</summary>
        static List<Net4> FreeBlocks(Net4 block, IEnumerable<Net4> used)
        {
            var free = new List<Net4>();
            long cursor = block.Addr;
            foreach (var u in used.OrderBy(n => n.Addr))
            {
                if (u.Addr > cursor) free.AddRange(RangeToCidr(cursor, u.Addr - 1L));
                cursor = Math.Max(cursor, (long)u.Last + 1);
            }
            if (cursor <= block.Last) free.AddRange(RangeToCidr(cursor, block.Last));
            return free;
        }

        /// <summary>Kiểm tra một tiền tố đề xuất, trả về chuỗi kết luận.</summary>
        static string CheckPrefix(string text, Net4 block, List<Alloc> allocated)
        {
            string note = "";
            if (!TryParseNet(text, true, out var net))
            {
                if (!TryParseNet(text, false, out net))             // thử lại, chấp nhận bit máy
                    return "KHÔNG HỢP LỆ (sai cú pháp địa chỉ hoặc tiền tố)";
                note = $"có bit máy, chuẩn hóa {net}; ";
            }
            if (!net.SubnetOf(block)) return note + $"NGOÀI KHỐI {block}";
            foreach (var a in allocated)
                if (net.Overlaps(a.Net)) return note + $"CHỒNG LẤN với {a.Name} ({a.Net})";
            return note + "HỢP LỆ, còn trống";
        }

        /// <summary>Địa chỉ khả dụng đầu tiên làm cổng mặc định; các thiết bị nhận các địa chỉ tiếp theo.</summary>
        static List<(string Dev, uint Ip)> AssignHosts(Net4 net, List<string> devices)
        {
            uint next = net.Addr + 1;
            var result = new List<(string, uint)> { ("gateway", next++) };
            foreach (var d in devices)
            {
                if (next >= net.Last) throw new InvalidOperationException($"Mạng {net} đã hết địa chỉ khi cấp cho {d}");
                result.Add((d, next++));
            }
            return result;
        }

        /// <summary>Xử lý một tệp kế hoạch; trả về false nếu kế hoạch không thực hiện được.</summary>
        public static bool RunPlan(string path)
        {
            using var doc = JsonDocument.Parse(File.ReadAllText(path, Encoding.UTF8));
            var root = doc.RootElement;
            Console.WriteLine($"===== Kế hoạch: {path} =====");
            var block = ParseNet(root.GetProperty("khoi_dia_chi").GetString()!);
            Console.WriteLine($"[ipam] Khối địa chỉ {block}: {block.Size} địa chỉ, " +
                              (IsPrivate(block) ? "riêng (RFC 1918)" : "công cộng"));
            var requests = root.GetProperty("yeu_cau").EnumerateArray()
                .Select(e => new Request(e.GetProperty("ten").GetString()!, e.GetProperty("so_may").GetInt32()))
                .ToList();
            List<Alloc> allocated;
            try
            {
                allocated = PlanVlsm(block, requests);
            }
            catch (InvalidOperationException e)
            {
                Console.WriteLine($"[ipam] LỖI: {e.Message}");
                return false;
            }

            Console.WriteLine("\n[ipam] 1) Chia mạng con (VLSM, yêu cầu lớn cấp trước)");
            Console.WriteLine($"  {"Tên",-14} {"Cần",4} {"Khả dụng",8} {"Mạng con",-16} {"Mặt nạ",-15} Dải khả dụng");
            foreach (var a in allocated)
            {
                var n = a.Net;     // bỏ địa chỉ mạng và địa chỉ quảng bá
                Console.WriteLine($"  {a.Name,-14} {a.Hosts,4} {n.Size - 2,8} {n,-16} {Str(n.Mask),-15} " +
                                  $"{Str(n.Addr + 1)} - {Str(n.Last - 1)}");
            }
            long used = allocated.Sum(a => a.Net.Size);
            Console.WriteLine($"  Đã dùng {used}/{block.Size} địa chỉ ({used * 100.0 / block.Size:F1}%)");

            Console.WriteLine("\n[ipam] 2) Khối còn trống");
            foreach (var f in FreeBlocks(block, allocated.Select(a => a.Net)))
                Console.WriteLine($"  {f,-17} ({f.Size} địa chỉ)");

            if (root.TryGetProperty("kiem_tra", out var checks) && checks.GetArrayLength() > 0)
            {
                Console.WriteLine("\n[ipam] 3) Kiểm tra tiền tố đề xuất");
                foreach (var t in checks.EnumerateArray())
                {
                    string text = t.GetString()!;
                    Console.WriteLine($"  {text,-17} -> {CheckPrefix(text, block, allocated)}");
                }
            }

            if (root.TryGetProperty("cap_phat", out var req) && req.ValueKind == JsonValueKind.Object)
            {
                string netName = req.GetProperty("mang").GetString()!;
                var net = allocated.First(a => a.Name == netName).Net;
                var devices = req.GetProperty("thiet_bi").EnumerateArray().Select(e => e.GetString()!).ToList();
                Console.WriteLine($"\n[ipam] 4) Cấp phát địa chỉ trong {netName} ({net})");
                foreach (var (dev, ip) in AssignHosts(net, devices))
                    Console.WriteLine($"  {dev,-8} {Str(ip)}/{net.Len}");
            }
            return true;
        }
    }
}
