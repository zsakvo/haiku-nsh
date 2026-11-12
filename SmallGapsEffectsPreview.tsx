import { host, mount } from "nsh";
import { SmallGapsFixture } from "./src/pages/fixtures/SmallGapsFixture";
mount(() => <SmallGapsFixture controls={host.params.controls === true} />);
