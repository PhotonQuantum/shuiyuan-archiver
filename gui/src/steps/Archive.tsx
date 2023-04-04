import {Alert, Center, Group, Loader, Stack, Text, useMantineTheme} from "@mantine/core";
import {appWindow} from "@tauri-apps/api/window";
import {useRecoilState, useRecoilValue, useSetRecoilState} from "recoil";
import {useEffect, useState} from "react";
import {archiveResultState, currentStep, maskUserState, rateLimitState, saveToState, topicMetaState} from "../states";
import {listen, UnlistenFn} from "@tauri-apps/api/event";
import {AlertCircle, Check, Clock, CloudDownload} from "tabler-icons-react";
import {DownloadEvent} from "../bindings";
import {archive} from "../commands";

type UnlistenStruct = {
    unsubscribe: UnlistenFn;
}

export const Archive = () => {
    const theme = useMantineTheme();

    const topicMeta = useRecoilValue(topicMetaState);
    const saveTo = useRecoilValue(saveToState);
    const maskUser = useRecoilValue(maskUserState);
    const setArchiveResult = useSetRecoilState(archiveResultState);
    const setStep = useSetRecoilState(currentStep);

    const [rateLimit, setRateLimit] = useRecoilState(rateLimitState);
    const [fetchMeta, setFetchMeta] = useState(true);
    const [pageDownloaded, setPageDownloaded] = useState(0);
    const [pageTotal, setPageTotal] = useState(0);
    const [resourcesDownloaded, setResourcesDownloaded] = useState(0);
    const [resourcesTotal, setResourcesTotal] = useState(0);

    const [channelRateLimit, setChannelRateLimit] = useState<UnlistenStruct | null>(null);
    const [channelProgress, setChannelProgress] = useState<UnlistenStruct | null>(null);

    useEffect(() => {
        archive(topicMeta!, saveTo, maskUser)
          .then(() => {
              setArchiveResult({success: true, message: ""});
              setStep(3);
          })
          .catch(resp => {
              setArchiveResult({success: false, message: resp as string});
              setStep(3);
          })
    }, [topicMeta, saveTo, maskUser]);

    useEffect(() => {
        listen<number>("rate-limit-event", (rateLimit) => {
            console.log("rateLimit", rateLimit);
            setRateLimit(rateLimit.payload);
        }).then(unsubscribe => {
            setChannelRateLimit({unsubscribe});
        }).catch(e => {
            console.error(e);
        });
        appWindow.listen<DownloadEvent>("progress-event", (progress) => {
            const payload = progress.payload;
            if (payload.kind === "post-chunks-total") {
                setFetchMeta(false);
                setPageTotal(payload.value!);
            } else if (payload.kind === "post-chunks-downloaded-inc") {
                setPageDownloaded((v) => v + 1);
            } else if (payload.kind === "resource-downloaded-inc") {
                setResourcesDownloaded((v) => v + 1);
            } else if (payload.kind === "resource-total-inc") {
                setResourcesTotal((v) => v + 1);
            }
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

    const stage = fetchMeta ? 0 : (pageDownloaded !== pageTotal ? 1 : 2);

    return (
      <>
          {rateLimit !== 0 &&
            <Alert icon={<AlertCircle size={16}/>} title="限流" color="orange">
                检测到您被限流！将在等待 {rateLimit} 秒后继续下载...
            </Alert>
          }
          <Center>
              <Group pt={rateLimit === 0 ? 80 : 40} spacing={"xl"}>
                  <CloudDownload size={80} strokeWidth={1}/>
                  <Stack spacing={"xs"}>
                      <Group spacing={"xs"}>
                          {stage === 0 ?
                            <Loader size={"sm"} variant={"dots"}/> :
                            <Check color={theme.colors.green[5]} size={21}/>
                          }
                          <Text size={"sm"}>读取元信息 ...{stage !== 0 && " 完成"}</Text>
                      </Group>
                      <Group spacing={"xs"}>
                          {stage === 0 ?
                            <Clock color={theme.colors.orange[5]} size={21}/> :
                            (stage === 1 ?
                              <Loader size={"sm"} variant={"dots"}/> :
                              <Check color={theme.colors.green[5]} size={21}/>)
                          }
                          <Text size={"sm"}>抓取页面
                              {(stage > 0) && (stage === 1 ? ` ... ${pageDownloaded}/${pageTotal}` : " ... 完成")}
                          </Text>
                      </Group>
                      <Group spacing={"xs"}>
                          {stage < 2 ?
                            <Clock color={theme.colors.orange[5]} size={21}/> :
                            <Loader size={"sm"} variant={"dots"}/>
                          }
                          <Text size={"sm"}>抓取资源
                              {stage === 2 && ` ... ${resourcesDownloaded}/${resourcesTotal}`}
                          </Text>
                      </Group>
                  </Stack>
              </Group>

          </Center>
      </>
    );
}