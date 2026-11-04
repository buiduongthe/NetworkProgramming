// Bài 7.3 – Server WebSocket tối giản và client thử (Mã giả 7.3, 7.4; Mục 7.3.2 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server  [host] [port]   (mặc định 0.0.0.0 8080)
//        dotnet run -- client  [host] [port]   (ClientWebSocket của .NET; đọc dòng từ bàn phím)
//        dotnet run -- kiemthu [host] [port]   (khung tự tạo, kể cả khung vi phạm đặc tả)
using Bai7_3;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
return mode switch
{
    "server" => await WsServer.RunAsync(rest),
    "client" => await WsClient.RunAsync(rest),
    "kiemthu" => WsKiemThu.Run(rest),
    _ => Usage(),
};

static int Usage()
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client|kiemthu [host] [port]");
    return 1;
}
