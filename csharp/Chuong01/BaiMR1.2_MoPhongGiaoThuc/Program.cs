// Bài mở rộng MR1.2 – Mô phỏng giao thức và kiểu truyền: TCP, UDP, unicast, broadcast, multicast, anycast.
//
// Bài mở rộng, không có trong giáo trình; minh họa trực quan Chương 1 (mô hình TCP/IP, tầng giao vận,
// các kiểu địa chỉ) trước khi lập trình socket thật ở các chương sau. Toàn bộ chạy trong bộ mô phỏng
// sự kiện rời rạc (Mang.cs): không mở socket, không cần quyền quản trị, kết quả tất định theo seed.
//
// Cách dùng:
//     dotnet run -- <kịch_bản> [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]
//     kịch_bản: tcp | udp | unicast | broadcast | multicast | anycast | tat_ca
//     --seed  hạt giống bộ sinh ngẫu nhiên (mặc định 7)
//     --loss  xác suất mất gói trên liên kết R2–R3, 0..1 (mặc định: 0; UDP: 0.2)
//     --doan  số đoạn dữ liệu TCP (mặc định 40)
//     --mat   các đoạn TCP bị ép mất; lặp số đoạn để mất nhiều lần (mặc định 20,20)
//     --html  ghi tệp HTML hoạt hình tự chứa (JavaScript thuần, không cần mạng)
using System.Globalization;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using BaiMR1_2;

Console.OutputEncoding = Encoding.UTF8;
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;

var titles = new Dictionary<string, string>
{
    ["tcp"] = "TCP – bắt tay, cửa sổ trượt, cwnd, mất gói, đóng FIN",
    ["udp"] = "UDP – datagram, mất gói, sai thứ tự, không gửi lại",
    ["unicast"] = "Unicast – một nguồn tới một đích",
    ["broadcast"] = "Broadcast – gửi tràn trong LAN, dừng ở router",
    ["multicast"] = "Multicast – IGMP, chỉ giao cho thành viên nhóm",
    ["anycast"] = "Anycast – máy phục vụ gần nhất, dự phòng khi hỏng",
};

// ---- đọc tham số (giống argparse của bản Python) ----
string? scenario = null;
long seed = 7; double loss = -1; int doan = 40; string mat = "20,20", html = "";
for (int i = 0; i < args.Length; i++)
{
    string Val() => i + 1 < args.Length ? args[++i] : throw new ArgumentException($"Thiếu giá trị cho {args[i]}");
    switch (args[i])
    {
        case "--seed": seed = long.Parse(Val()); break;
        case "--loss": loss = double.Parse(Val(), CultureInfo.InvariantCulture); break;
        case "--doan": doan = int.Parse(Val()); break;
        case "--mat": mat = Val(); break;
        case "--html": html = Val(); break;
        default: scenario = args[i]; break;
    }
}
if (scenario == null || !(titles.ContainsKey(scenario) || scenario == "tat_ca"))
{
    Console.WriteLine("Cách dùng: dotnet run -- tcp|udp|unicast|broadcast|multicast|anycast|tat_ca " +
                      "[--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]");
    return 2;
}

var names = scenario == "tat_ca" ? titles.Keys.ToList() : new List<string> { scenario };
var runs = names.Select(RunOne).ToList();
if (html != "") WriteHtml(html, runs);
return 0;

Dictionary<string, object> RunOne(string name)
{
    Console.WriteLine($"=== Kịch bản {name} (seed {seed}) ===");
    Console.WriteLine(titles[name]);
    var net = new Net(seed);
    bool lossSet = loss >= 0;
    if (lossSet) net.L[("R2", "R3")].Loss = (int)(loss * 1000 + 0.5);
    var run = new Dictionary<string, object> { ["id"] = name, ["title"] = titles[name] };
    switch (name)
    {
        case "tcp":
            var drops = mat.Split(',').Where(x => x.Trim() != "").Select(x => int.Parse(x.Trim())).ToList();
            Console.WriteLine($"Truyền {doan} đoạn × {TcpScenario.Mss} byte từ H1 tới H5; ép mất đoạn: " +
                              (drops.Count > 0 ? string.Join(", ", drops.Select(d => "#" + d)) : "không"));
            var sc = new TcpScenario(net, doan, drops);
            sc.Start();
            net.Run();
            sc.Report();
            run["cwnd"] = sc.CwndLog;
            break;
        case "udp": KichBan.Udp(net, lossSet); break;
        case "unicast": KichBan.Unicast(net); break;
        case "broadcast": KichBan.Broadcast(net); break;
        case "multicast": KichBan.Multicast(net); break;
        case "anycast": KichBan.Anycast(net); break;
    }
    Console.WriteLine();
    net.Summary();
    Console.WriteLine();
    run["hops"] = net.Hops;
    run["notes"] = net.Notes;
    run["changes"] = net.Changes;
    return run;
}

// Chèn dữ liệu JSON của các lần chạy vào khung trang hoạt hình (nhúng trong tệp .dll).
void WriteHtml(string path, List<Dictionary<string, object>> runList)
{
    var data = new Dictionary<string, object>
    {
        ["nodes"] = Net.Nodes.Select(n => new Dictionary<string, object>
            { ["name"] = n.Name, ["kind"] = n.Kind, ["ip"] = n.Ip, ["x"] = n.X, ["y"] = n.Y }).ToList(),
        ["links"] = Net.Links.Select(l => new Dictionary<string, object>
            { ["a"] = l.A, ["b"] = l.B, ["cost"] = l.Cost }).ToList(),
        ["runs"] = runList,
    };
    using var res = typeof(Net).Assembly.GetManifestResourceStream("khung_hoat_hinh.html")!;
    string tpl = new StreamReader(res, Encoding.UTF8).ReadToEnd();
    // UnsafeRelaxedJsonEscaping: giữ nguyên chữ tiếng Việt; "</" được thay để không đóng thẻ <script> sớm
    var opt = new JsonSerializerOptions { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    string js = JsonSerializer.Serialize(data, opt).Replace("</", "<\\/");
    File.WriteAllText(path, tpl.Replace("{{.Data}}", js), new UTF8Encoding(false));
    Console.WriteLine($"Đã ghi hoạt hình HTML: {path}");
}
