import { useChannelPointsHudModel } from "./channelPointsHud/model";
import { ChannelPointsHudView } from "./channelPointsHud/HudView";
import "./ChannelPointsHud.css";

export function ChannelPointsHud() {
  const model = useChannelPointsHudModel();
  if (!model.channel || !model.chip || !model.overlay || !model.host) {
    return <div className="points-hud points-hud--empty" />;
  }
  return (
    <ChannelPointsHudView
      {...model}
      channel={model.channel}
      chip={model.chip}
      overlay={model.overlay}
    />
  );
}

