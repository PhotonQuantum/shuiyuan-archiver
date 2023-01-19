import {AppShell, Container, Stepper} from "@mantine/core";
import {Login} from "./steps/Login";
import {useRecoilState} from "recoil";
import {currentStep} from "./states";
import {Config} from "./steps/Config";
import {Archive} from "./steps/Archive";
import {Finish} from "./steps/Finish";

export const Main = () => {
    const [step, _] = useRecoilState(currentStep);
    return (
        <AppShell>
            <Stepper active={step}>
                <Stepper.Step label={"登录"} description={"登录社区"}>
                    <Container>
                        <Login/>
                    </Container>
                </Stepper.Step>
                <Stepper.Step label={"配置"} description={"选择贴子"}>
                    <Container>
                        <Config/>
                    </Container>
                </Stepper.Step>
                <Stepper.Step label={"存档"} description={"下载贴子"}>
                    <Container>
                        <Archive/>
                    </Container>
                </Stepper.Step>
                <Stepper.Completed>
                    <Container>
                        <Finish/>
                    </Container>
                </Stepper.Completed>
            </Stepper>
        </AppShell>
    )
}