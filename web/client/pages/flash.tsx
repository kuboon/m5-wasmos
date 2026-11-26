import { type Handle } from "@remix-run/component";

import { Flasher } from "../islands/flasher.tsx";
import { firmwarePath } from "../routes.ts";

export const title = "書き込み — m5-wasmos";
export const description =
  "ブラウザから M5Stack Tab5 に m5-wasmos を書き込み、Wi-Fi を設定する";

export interface FlashProps {
  /** firmware.json の要約（無ければまだ公開されていない） */
  info: string | null;
}

export default function FlashPage(handle: Handle<FlashProps>) {
  return () => (
    <>
      <h1>実機に書き込む</h1>
      <p>
        PC の Chrome / Edge から USB で書き込みます（Web Serial
        を使います。Safari / Firefox は未対応）。書き込むとアプリは本体が Wi-Fi
        経由でこのサイトから取ってきます。
      </p>
      <Flasher firmwareUrl={firmwarePath} info={handle.props.info} />
    </>
  );
}
