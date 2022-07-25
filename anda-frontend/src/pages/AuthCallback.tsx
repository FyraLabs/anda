import { useHandleSignInCallback } from "@logto/react";
import { useNavigate } from "@tanstack/react-location";

const AuthCallback = () => {
  const navigate = useNavigate();
  const { isLoading } = useHandleSignInCallback(() => {
    navigate({ to: "/app/home", replace: true });
  });

  if (isLoading) {
    return <div>Redirecting...</div>;
  }

  return <></>;
};

export default AuthCallback;
