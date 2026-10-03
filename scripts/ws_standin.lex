import "std.net" as net
import "std.str" as str
import "std.json" as json
import "std.list" as list
import "std.int" as int
import "std.io" as io
import "std.datetime" as datetime

fn name_of(conn :: WsConn) -> Str { "" }

fn answer(id :: Str, action :: Str) -> [time] Str {
  if action == "BootNotification" {
    str.concat("[3,\"", str.concat(id, "\",{\"status\":\"Accepted\",\"currentTime\":\"2026-01-01T00:00:00.000Z\",\"interval\":30}]"))
  } else {
    if action == "Heartbeat" {
      str.concat("[3,\"", str.concat(id, "\",{\"currentTime\":\"2026-01-01T00:00:00.000Z\"}]"))
    } else {
      str.concat("[3,\"", str.concat(id, "\",{}]"))
    }
  }
}

fn on_message(c :: WsConn, msg :: WsMessage) -> [time] WsAction {
  match msg {
    WsText(body) => match json.parse(body) {
      Err(_) => WsNoOp,
      Ok(v) => handle(v),
    },
    _ => WsNoOp,
  }
}

fn handle(v :: List[Str]) -> [time] WsAction {
  let rest := list.tail(v)
  match list.head(rest) {
    None => WsNoOp,
    Some(id) => match list.head(list.tail(rest)) {
      None => WsNoOp,
      Some(action) => WsSend(answer(id, action)),
    },
  }
}

fn main() -> [io, net, concurrent, time] Unit {
  net.serve_ws_fn_actor(9201, "ocpp1.6", name_of, on_message)
}
