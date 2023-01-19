import {Alert, Center, Group, Loader, Stack, Text} from "@mantine/core";
import {appWindow} from "@tauri-apps/api/window";
import {useRecoilValue, useSetRecoilState} from "recoil";
import {useEffect, useState} from "react";
import {invoke} from "@tauri-apps/api";
import {archiveResultState, currentStep, maskUserState, saveAtState, tokenState, topicState} from "../states";
import {UnlistenFn} from "@tauri-apps/api/event";
import {AlertCircle} from "tabler-icons-react";

type UnlistenStruct = {
    unsubscribe: UnlistenFn;
}

export const Archive = () => {
    const token = useRecoilValue(tokenState);
    const topic = useRecoilValue(topicState);
    const saveAt = useRecoilValue(saveAtState);
    const maskUser = useRecoilValue(maskUserState);
    const setArchiveResult = useSetRecoilState(archiveResultState);
    const setStep = useSetRecoilState(currentStep);

    const [rateLimit, setRateLimit] = useState(0);
    const [progress, setProgress] = useState("");

    const [channelRateLimit, setChannelRateLimit] = useState<UnlistenStruct | null>(null);
    const [channelProgress, setChannelProgress] = useState<UnlistenStruct | null>(null);

    useEffect(() => {
        invoke("archive", {token, topic, saveAt, maskUser})
            .then(() => {
                setArchiveResult({success: true, message: ""});
                setStep(3);
            })
            .catch(resp => {
                setArchiveResult({success: false, message: resp as string});
                setStep(3);
            })
    }, [topic, saveAt, maskUser]);

    useEffect(() => {
        appWindow.listen<number>("rate-limit-event", (rateLimit) => {
            console.log("rateLimit", rateLimit);
            setRateLimit(rateLimit.payload);
        }).then(unsubscribe => {
            setChannelRateLimit({unsubscribe});
        }).catch(e => {
            console.error(e);
        });
        appWindow.listen<string>("progress-event", (progress) => {
            setProgress(progress.payload);
        }).then(unsubscribe => {
            setChannelProgress({unsubscribe});
        }).catch(e => {
            console.error(e);
        });

        return () => {
            console.log("start unsubscribing channels", channelProgress, channelRateLimit);
            if (channelRateLimit !== null) {
                console.log("unsubscribe rate-limit");
                channelRateLimit.unsubscribe();
            }
            if (channelProgress !== null) {
                console.log("unsubscribe progress");
                channelProgress.unsubscribe();
            }
        };
    }, []);

    return (
        <>
            {rateLimit !== 0 &&
                <Alert icon={<AlertCircle size={16}/>} title="限流" color="orange">
                    检测到您被限流！将在等待 {rateLimit} 秒后继续下载...
                </Alert>
            }
            <Center>
                <Group pt={rateLimit === 0 ? 80 : 40}>
                    <Loader size="xl"/>
                    <Stack spacing={0}>
                        <Text size={"xl"}>存档中</Text>
                        <Text size={"sm"}>{progress}</Text>
                    </Stack>
                </Group>
            </Center>
        </>
    );
}