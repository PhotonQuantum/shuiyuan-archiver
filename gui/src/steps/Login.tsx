import {useRecoilState, useSetRecoilState} from "recoil";
import {currentStep, tokenState} from "../states";
import {Button, Center, Group, Loader, Space, Stack, Text, Textarea, useMantineTheme} from "@mantine/core";
import {useEffect, useState} from "react";
import {loginWithToken, openBrowser, tokenFromOauth} from "../commands";
import {listen, UnlistenFn} from "@tauri-apps/api/event";
import {AlertTriangle} from "tabler-icons-react";

type UnlistenStruct = {
    unsubscribe: UnlistenFn;
}

enum OpenState {
    NotOpened,
    Plain,
    Callback
}

export const Login = () => {
    const theme = useMantineTheme();

    const [loading, setLoading] = useState(false);
    const [opened, setOpened] = useState(OpenState.NotOpened);
    const [token, setToken] = useRecoilState(tokenState);
    const setCurrentStep = useSetRecoilState(currentStep);
    const [OAuthKey, setOAuthKey] = useState("");
    const [keyError, setKeyError] = useState("");
    const [channelUpdateToken, setChannelUpdateToken] = useState<UnlistenStruct | null>(null);
    const enabled = OAuthKey.trim().length > 0;

    const validateToken = async (token: string) => {
        setLoading(true);
        try {
            await loginWithToken(token);
            setCurrentStep(1);
        } catch (e) {
            setToken('');
        }
        setLoading(false);
    }

    const loginWithOAuthKey = async (key: string) => {
        const token = await tokenFromOauth(key.replaceAll("\n", ""));
        if (token !== '') {
            setToken(token);
        } else {
            setKeyError("无效的授权码");
        }
    }

    useEffect(() => {
        // tauri listen for event `update_token`
        listen<string>("update-token", (token) => {
            loginWithOAuthKey(token.payload).then(_ => {
            })
        }).then(unsubscribe => {
            setChannelUpdateToken({unsubscribe});
        }).catch(e => {
            console.error(e);
        });

        return () => {
            if (channelUpdateToken !== null) {
                channelUpdateToken.unsubscribe();
            }
        };
    }, [])

    useEffect(() => {
        if (token !== '') {
            validateToken(token).then(_ => {
            });
        }
    }, [token]);

    return (
        <>{loading ?
            <Center pt={130}>
                <Group>
                    <Loader/>
                    <Text>正在登录...</Text>
                </Group>
            </Center> :
            (opened === OpenState.NotOpened ?
                    <Stack pt={70}>
                        <Text align={"center"}>为了存档水源贴子，我们需要您水源账号的只读权限</Text>
                        <Button onClick={() => {
                            openBrowser().then((use_callback) => {
                                if (use_callback) {
                                    setOpened(OpenState.Callback);
                                } else {
                                    setOpened(OpenState.Plain);
                                }
                            });
                        }}>授权</Button>
                    </Stack> : (opened === OpenState.Plain ? <Stack pt={40}>
                            <Text align={"center"}>请在弹出的网页授权后，按照指引将授权码粘贴在下方</Text>
                            <Textarea placeholder="授权码" required
                                      value={OAuthKey}
                                      onChange={ev => setOAuthKey(ev.currentTarget.value)}
                                      error={keyError}
                            />
                            <Space/>
                            <Button disabled={!enabled} onClick={() => {
                                loginWithOAuthKey(OAuthKey).then(_ => {
                                })
                            }}>登录</Button>
                        </Stack> : <Stack pt={70} spacing={"xl"}>
                            <Text align={"center"}>请按照指引在弹出的网页完成授权</Text>
                            <Center>
                                <Group spacing={"xs"}>
                                    {keyError === "" ? (
                                        <>
                                            <Loader size={"sm"}/>
                                            <Text>正在等待授权...</Text>
                                        </>
                                    ) : (
                                        <>
                                            <AlertTriangle color={theme.colors.orange[5]} strokeWidth={1} size={28}/>
                                            <Text>{keyError}</Text>
                                        </>
                                    )}
                                </Group>
                            </Center>

                        </Stack>
                    )
            )}</>
    )
}