// Online play (see src/net.rs): a Supabase Realtime broadcast channel per room.
// Messages are short text lines; presence tells who's in the room. Configure the project
// in net-config.js (the URL and the anon key are public by design).
(function () {
  const cfg = window.BB_NET || {};
  const me = Math.random().toString(36).slice(2, 10);
  let client = null, channel = null, status = 0, members = [];
  let inbox = [], out = new Uint8Array(0);
  // status: 0 off, 1 joining, 2 in the room, 3 error, 4 not configured
  function ensure() {
    if (client) return true;
    if (!cfg.url || !cfg.anonKey || !window.supabase) { status = 4; return false; }
    client = window.supabase.createClient(cfg.url, cfg.anonKey, { realtime: { params: { eventsPerSecond: 20 } } });
    return true;
  }
  function leave() {
    if (channel && client) client.removeChannel(channel);
    channel = null; status = 0; members = []; inbox = [];
  }
  const text = (p, l) => new TextDecoder().decode(new Uint8Array(wasm_memory.buffer, p, l));
  miniquad_add_plugin({
    name: "bb_net",
    version: 1,
    register_plugin(importObject) {
      const env = importObject.env;
      env.bb_net_join = function (p, l) {
        const room = text(p, l);
        leave();
        if (!ensure()) return;
        status = 1;
        channel = client.channel("block-brawl-" + room, { config: { broadcast: { self: false, ack: false }, presence: { key: me } } });
        channel.on("broadcast", { event: "m" }, (m) => { if (m.payload && typeof m.payload.t === "string") inbox.push(m.payload.t); });
        channel.on("presence", { event: "sync" }, () => { members = Object.keys(channel.presenceState()).sort(); });
        channel.subscribe(async (s) => {
          if (s === "SUBSCRIBED") { status = 2; try { await channel.track({ at: Date.now() }); } catch (e) { console.warn(e); } }
          else if (s === "CHANNEL_ERROR" || s === "TIMED_OUT") status = 3;
        });
      };
      env.bb_net_send = function (p, l) {
        if (channel && status === 2) channel.send({ type: "broadcast", event: "m", payload: { t: text(p, l) } });
      };
      env.bb_net_poll = function () {
        if (!inbox.length) return -1;
        out = new TextEncoder().encode(inbox.shift());
        return out.length;
      };
      env.bb_net_take = function (p) {
        new Uint8Array(wasm_memory.buffer, p, out.length).set(out);
        out = new Uint8Array(0);
      };
      env.bb_net_status = () => status;
      env.bb_net_peers = () => members.length;
      // The host (who picks the seed) is whoever sorts first among those present.
      env.bb_net_is_host = () => (members.length > 0 && members[0] === me ? 1 : 0);
      env.bb_net_leave = () => leave();
    },
  });
})();
