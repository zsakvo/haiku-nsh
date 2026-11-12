import { host, mount } from "nsh";
import { SmallGapsFixture } from "./src/pages/fixtures/SmallGapsFixture";
mount(() => <SmallGapsFixture grid controls={host.params.controls === true} />);
