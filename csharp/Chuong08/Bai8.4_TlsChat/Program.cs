// Bài 8.4 – Bổ sung TLS và xác thực token cho hệ thống chat (Mã giả 8.6; Mục 8.4.5 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]                       (mặc định 0.0.0.0 9200)
//        dotnet run -- client <host> <port> <tệp_token> [tệp_CA]
//        dotnet run -- token <sub> <tệp_token> [aud] [thời_hạn_giây]
// Cấu hình server: TLS_CERT (tệp PKCS12 chat.p12), TLS_KEY_PASS (mật khẩu của tệp), CHAT_JWT_KEY (khóa ký JWT)
using System.Globalization;
using Bai8_4;

CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
Console.OutputEncoding = System.Text.Encoding.UTF8;
if (args.Length == 0 || args[0] is not ("server" or "client" or "token"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client|token ...");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] switch
{
    "server" => await ChatServer.Run(rest),
    "client" => await ChatClient.Run(rest),
    _ => Jwt.TokenTool(rest)
};
